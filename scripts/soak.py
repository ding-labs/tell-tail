#!/usr/bin/env python3
"""Seeded bounded soak runs with exact counts and process-tree resource samples."""
import argparse
from collections import deque
import json
import hashlib
import os
import pathlib
import platform
import pty
import re
import select
import signal
import statistics
import struct
import subprocess
import tempfile
import termios
import threading
import time
import fcntl
from soak_support import Counts, LEVELS, ObservationClock, make_batch, resources

ROOT = pathlib.Path(__file__).resolve().parents[1]


def optional_command(command):
    try:
        return subprocess.check_output(command, cwd=ROOT, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def metrics_report(text):
    assert text.endswith('\n'), 'incomplete metrics exposition'
    pairs = re.findall(r'^tell_tail_records_total\{level="(\w+)"\} (\d+)$', text, re.M)
    assert len(pairs) == 7, 'missing or duplicate severity series'
    levels = dict(pairs)
    assert set(levels) == set(LEVELS), levels
    levels = {key: int(value) for key, value in levels.items()}
    def value(name):
        match = re.search(r'^tell_tail_' + name + r' ([^\n]+)$', text, re.M)
        assert match, name
        return float(match[1])
    return {'totals': {'records': sum(levels.values()), 'levels': levels,
                      'parse_failures': int(value('parse_failures_total')),
                      'oversized_records': int(value('oversized_records_total'))},
            'bytes_read': int(value('bytes_total')), 'pending_bytes': int(value('pending_bytes')),
            'skipped_bytes': int(value('skipped_bytes_total')),
            'discarded_partial_records': int(value('discarded_partial_records_total')),
            'source_up': value('source_up'), 'final': value('running') == 0,
            'observed_at_unix_seconds': value('snapshot_timestamp_seconds')}


def text_report(tail):
    marker = b'final source=stdin'
    assert marker in tail, 'missing final terminal report'
    final = tail.rsplit(marker, 1)[1]
    def value(name):
        match = re.search(rb'\b' + name.encode() + rb'=(\d+)(?:\s|$)', final)
        assert match, (name, final)
        return int(match[1])
    return {'totals': {'records': value('records'),
                      'levels': {level: value(level) for level in LEVELS},
                      'parse_failures': value('parse_failures'),
                      'oversized_records': value('oversized')},
            'bytes_read': value('bytes'), 'pending_bytes': value('pending_bytes'),
            'skipped_bytes': 0, 'discarded_partial_records': 0}


def run(args):
    payload, batch, digest = make_batch(args.seed, args.profile, args.max_record_bytes)
    copies = getattr(args, 'batch_copies', 1)
    assert len(payload) * copies <= 64 * 1024**2, 'producer batch exceeds the 64 MiB harness limit'
    if copies != 1:
        original = batch
        batch = Counts()
        for _ in range(copies):
            batch.add(original)
        payload *= copies
        digest = hashlib.sha256(payload).hexdigest()
    assert not args.records or args.records % batch.records == 0, 'workload must contain whole producer batches'
    expected = Counts()
    finished = threading.Event()
    latest = [None]
    errors = []
    terminal_tail = bytearray()
    error_tail = bytearray()
    ages = [0] * 201  # 10 ms bins plus an overflow bin; constant memory.
    observed = [0]
    max_age = [0.0]
    with tempfile.TemporaryDirectory(prefix='tell-tail-soak-') as directory:
        directory = pathlib.Path(directory)
        log = directory / 'input.log'
        metrics = directory / 'metrics.prom'
        if args.input == 'file':
            log.touch()
        command = [str(args.binary)]
        if args.input == 'file':
            command += [str(log), '--from', 'start']
        command += ['--output', args.output, '--window', str(args.window),
                    '--max-record-bytes', str(args.max_record_bytes)]
        if args.output == 'prometheus':
            command += ['--metrics-file', str(metrics)]
        master = slave = None
        if args.output == 'tui':
            master, slave = pty.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 26, 100, 0, 0))
        def setup():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        child = subprocess.Popen(command, stdin=subprocess.PIPE if args.input == 'stdin' else subprocess.DEVNULL,
                                 stdout=slave if master is not None else subprocess.PIPE,
                                 stderr=subprocess.PIPE, preexec_fn=setup if master is not None else None,
                                 env={**os.environ, 'TERM': 'xterm-256color'})
        if slave is not None:
            os.close(slave)
        started = time.monotonic()
        continuity = ObservationClock(started, time.time())
        deadline = started + args.seconds
        def update(report):
            latest[0] = report
            age = max(0, time.time() - report['observed_at_unix_seconds'])
            ages[min(int(age * 100), 200)] += 1
            max_age[0] = max(max_age[0], age)
            observed[0] += 1
        def observe():
            try:
                if args.output == 'json':
                    for line in child.stdout:
                        assert len(line) <= 65536, 'report exceeds harness buffer budget'
                        update(json.loads(line))
                elif args.output == 'prometheus':
                    previous = None
                    while not finished.is_set():
                        if metrics.exists():
                            text = metrics.read_text()
                            if text != previous:
                                update(metrics_report(text))
                                previous = text
                        finished.wait(.02)
                else:
                    query_tail = b''
                    next_action = started + 5
                    action = 0
                    while not finished.is_set():
                        if getattr(args, 'exercise_terminal', False) and time.monotonic() >= next_action and child.poll() is None:
                            if action < 4:
                                os.write(master, (b' ', b'?', b'?', b' ')[action])
                            else:
                                height, width = (10, 40) if action == 4 else (26, 100)
                                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack('HHHH', height, width, 0, 0))
                            result['terminal_actions'] += 1
                            action = (action + 1) % 6
                            next_action = time.monotonic() + (55 if action == 0 else 1)
                        if select.select([master], [], [], .02)[0]:
                            try:
                                chunk = os.read(master, 65536)
                            except OSError:
                                break
                            if not chunk:
                                break
                            if b'\x1b[6n' in query_tail + chunk:
                                os.write(master, b'\x1b[1;1R')
                            query_tail = chunk[-3:]
                            terminal_tail.extend(chunk)
                            del terminal_tail[:-32768]
            except Exception as error:
                errors.append(repr(error))
        def diagnostics():
            for chunk in iter(lambda: child.stderr.read(4096), b''):
                error_tail.extend(chunk)
                del error_tail[:-32768]
        def wait_for(predicate):
            until = time.monotonic() + 5
            while time.monotonic() < until:
                if latest[0] is not None and predicate(latest[0]):
                    return
                assert child.poll() is None and not errors, (child.poll(), errors, bytes(error_tail))
                time.sleep(.01)
            raise AssertionError('expected snapshot acknowledgment was not received')
        def produce():
            cpu_started = time.thread_time()
            write_seconds = 0.0
            try:
                stream = child.stdin if args.input == 'stdin' else log.open('ab', buffering=0)
                file_bytes = 0
                rotations = 0
                try:
                    while time.monotonic() < deadline and not finished.is_set():
                        if args.records and expected.records >= args.records:
                            break
                        if args.profile == 'idle':
                            finished.wait(.1)
                            continue
                        elapsed = time.monotonic() - started
                        pacing = expected.records / args.rate if args.rate else 0
                        if args.input == 'file':
                            pacing = max(pacing, expected.bytes_read / (args.file_gib_per_day * 1024**3 / 86400))
                        if args.profile == 'burst' and elapsed % 5 >= 2:
                            pacing = max(pacing, elapsed + .1)
                        if pacing > elapsed:
                            finished.wait(min(pacing - elapsed, .1))
                            continue
                        write_started = time.monotonic()
                        stream.write(payload)
                        stream.flush()
                        write_seconds += time.monotonic() - write_started
                        expected.add(batch)
                        file_bytes += len(payload)
                        if args.input == 'file' and file_bytes >= args.rotate_bytes:
                            wait_for(lambda s: s['totals']['records'] == expected.records)
                            partial = b'INFO unfinished' if args.profile == 'rotation' else b''
                            if partial:
                                stream.write(partial)
                                expected.bytes_read += len(partial)
                                wait_for(lambda s: s['pending_bytes'] == len(partial))
                            stream.close()
                            log.rename(directory / 'old.log')
                            if partial:
                                wait_for(lambda s: s.get('state') == 'waiting' or s.get('source_up') == 0)
                            log.touch()
                            stream = log.open('ab', buffering=0)
                            if partial:
                                expected.skipped_bytes += len(partial)
                                expected.discarded_partial_records += 1
                                wait_for(lambda s: s['discarded_partial_records'] == expected.discarded_partial_records)
                            (directory / 'old.log').unlink()
                            file_bytes = 0
                            rotations += 1
                    if args.input == 'file':
                        wait_for(lambda s: s['totals']['records'] == expected.records and s['pending_bytes'] == 0)
                        result['shutdown_requested_at'] = time.monotonic()
                        child.send_signal(signal.SIGTERM)
                finally:
                    result.setdefault('shutdown_requested_at', time.monotonic())
                    stream.close()
                result['rotations'] = rotations
            except Exception as error:
                errors.append(repr(error))
            finally:
                result['producer_cpu_seconds'] = time.thread_time() - cpu_started
                result['producer_write_wall_seconds'] = write_seconds
        result = {'rotations': 0, 'terminal_actions': 0}
        observer = threading.Thread(target=observe, daemon=True)
        diagnostic = threading.Thread(target=diagnostics, daemon=True)
        producer = threading.Thread(target=produce, daemon=True)
        observer.start()
        diagnostic.start()
        producer.start()
        samples = 0
        peak = peak_cpu = 0.0
        peak_fds = None
        peak_processes = 0
        recent = deque(maxlen=600)
        blocks = []
        next_sample = started
        next_progress = started + 60
        next_block = started + 600
        try:
            while child.poll() is None:
                now = time.monotonic()
                continuity.observe(now, time.time())
                assert not errors, errors
                assert now < deadline + 12, 'soak run timed out'
                if now >= next_sample:
                    sample = resources(child.pid, include_fds=samples < 3 or samples % 60 == 0)
                    if sample:
                        samples += 1
                        peak = max(peak, sample['rss_mib'])
                        peak_cpu = max(peak_cpu, sample['sampled_cpu_seconds'])
                        peak_processes = max(peak_processes, sample['processes'])
                        if sample['open_descriptors'] is not None:
                            peak_fds = max(peak_fds or 0, sample['open_descriptors'])
                            assert peak_fds <= 64, 'process-tree descriptor budget exceeded'
                        recent.append(sample['rss_mib'])
                        assert peak < args.rss_limit_mib, f'combined RSS exceeds budget: {peak:.2f} MiB'
                    size = sum(p.stat().st_size for p in directory.iterdir() if p.is_file())
                    assert size < args.disk_limit_mib * 1024**2, 'harness disk budget exceeded'
                    next_sample = now + 1
                if now >= next_block and recent:
                    blocks.append(statistics.median(recent))
                    recent.clear()  # wall-clock blocks must not overlap when sampling drifts
                    next_block = now + 600
                    if len(blocks) > 2:
                        baseline = blocks[1]  # first complete post-warmup block
                        assert blocks[-1] <= baseline + max(4, baseline * .2), 'RSS plateau check failed'
                if now >= next_progress:
                    print(json.dumps({'progress_seconds': round(now - started), 'records': expected.records,
                                      'peak_sampled_rss_mib': peak}), file=__import__('sys').stderr, flush=True)
                    next_progress = now + 60
                time.sleep(.02)
            exited_at = time.monotonic()
            continuity.observe(exited_at, time.time())
            producer.join(timeout=2)
            assert not producer.is_alive() and not errors, errors
            shutdown_at = result.pop('shutdown_requested_at')
            result['shutdown_observed_seconds'] = max(0, exited_at - shutdown_at)
            result['production_seconds'] = shutdown_at - started
            result['producer_cpu_fraction'] = result['producer_cpu_seconds'] / result['production_seconds']
            result['producer_saturation_warning'] = result['producer_cpu_fraction'] > .75
            assert child.returncode == (143 if args.input == 'file' else 0), (child.returncode, bytes(error_tail))
            if args.output == 'prometheus':
                report = metrics_report(metrics.read_text())
                assert report['final']
            elif args.output == 'tui':
                # The PTY reader drains through EOF, including the final summary.
                observer.join(timeout=1)
                report = text_report(bytes(terminal_tail))
            else:
                observer.join(timeout=1)
                report = latest[0]
                assert report['kind'] == 'final'
            expected.verify(report)
            if args.records:
                assert expected.records == args.records, 'fixed workload did not finish before the duration limit'
            elapsed = time.monotonic() - started
            threshold = observed[0] * .95
            cumulative = 0
            p95 = None
            for index, count in enumerate(ages):
                cumulative += count
                if cumulative >= threshold and observed[0]:
                    p95 = (index + 1) * 10 if index < 200 else None
                    break
            git_status = optional_command(['git', 'status', '--porcelain'])
            result.update({'commit': optional_command(['git', 'rev-parse', 'HEAD']),
                           'platform': platform.platform(), 'architecture': platform.machine(),
                           'workspace_toolchain': optional_command(['rustc', '--version']),
                           'workspace_dirty': None if git_status is None else bool(git_status),
                           'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                           'binary': str(args.binary), 'input': args.input, 'mode': args.output,
                           'exercise_terminal': getattr(args, 'exercise_terminal', False),
                           'batch_copies': copies,
                           'profile': args.profile, 'seed': args.seed, 'fixture_sha256': digest,
                           'configuration': {key: getattr(args, key) for key in
                               ('seconds', 'records', 'rate', 'window', 'max_record_bytes', 'rss_limit_mib',
                                'disk_limit_mib', 'file_gib_per_day', 'rotate_bytes')},
                           'records': expected.records, 'bytes': expected.bytes_read,
                           'expected': expected.__dict__, 'actual': report,
                           'seconds': elapsed, 'records_per_second': round(expected.records / elapsed),
                           'resource_samples': samples, 'sample_interval_seconds': 1,
                           'max_observation_gap_seconds': continuity.maximum_gap,
                           'peak_sampled_rss_mib': peak if samples else None,
                           'rss_10minute_medians_mib': blocks,
                           'peak_sampled_cpu_seconds': peak_cpu if samples else None,
                           'peak_sampled_descriptors': peak_fds, 'peak_sampled_processes': peak_processes,
                           'snapshot_age_p95_upper_ms': p95,
                           'max_snapshot_age_seconds': max_age[0] if observed[0] else None})
            return result
        except Exception as error:
            error.soak_details = {'expected': expected.__dict__, 'latest_snapshot': latest[0],
                                  'peak_sampled_rss_mib': peak if samples else None,
                                  'stderr_tail': error_tail.decode(errors='replace'),
                                  'max_observation_gap_seconds': continuity.maximum_gap,
                                  'elapsed_seconds': time.monotonic() - started}
            raise
        finally:
            finished.set()
            if child.poll() is None:
                child.kill()
                child.wait(timeout=3)
            if master is not None:
                os.close(master)
            producer.join(timeout=2)
            observer.join(timeout=2)
            diagnostic.join(timeout=2)
            if child.stdout:
                child.stdout.close()
            child.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seconds', type=float, default=60)
    parser.add_argument('--records', type=int, default=0, help='stop after this many records (multiple of 100); 0 uses duration')
    parser.add_argument('--batch-copies', type=int, default=1, help='repeat each 100-record producer batch; throughput measurements only')
    parser.add_argument('--binary', type=pathlib.Path, default=ROOT / 'target/release/tell-tail')
    parser.add_argument('--input', choices=['stdin', 'file'], default='stdin')
    parser.add_argument('--output', choices=['json', 'prometheus', 'tui'], default='json')
    parser.add_argument('--tui', action='store_true', help='alias for --output tui')
    parser.add_argument('--exercise-terminal', action='store_true', help='periodically freeze, show help, resume, and resize the TUI')
    parser.add_argument('--profile', choices=['sustained', 'mixed', 'burst', 'rotation', 'idle'], default='sustained')
    parser.add_argument('--seed', type=int, default=1)
    parser.add_argument('--rate', type=float, default=1000, help='records/sec; 0 is unlimited')
    parser.add_argument('--max-record-bytes', type=int, default=1024**2)
    parser.add_argument('--window', type=int, default=60)
    parser.add_argument('--rss-limit-mib', type=float, default=64)
    parser.add_argument('--disk-limit-mib', type=float, default=256)
    parser.add_argument('--file-gib-per-day', type=float, default=10)
    parser.add_argument('--rotate-bytes', type=int, default=8 * 1024**2)
    parser.add_argument('--report', type=pathlib.Path)
    args = parser.parse_args()
    if args.tui:
        args.output = 'tui'
    if not (0 < args.seconds <= 86400 and 1 <= args.batch_copies <= 1024
            and args.records >= 0 and args.records % (100 * args.batch_copies) == 0
            and args.rate >= 0 and args.file_gib_per_day > 0
            and 1 <= args.window <= 3600 and 1 <= args.max_record_bytes <= 16 * 1024**2
            and args.rss_limit_mib > 0 and args.disk_limit_mib > 0 and args.rotate_bytes > 0):
        parser.error('invalid duration, rate, window, or resource budget')
    if args.input == 'file' and args.output == 'tui':
        parser.error('the TUI soak uses stdin; file rotation needs machine-report acknowledgments')
    if args.exercise_terminal and args.output != 'tui':
        parser.error('terminal interactions require TUI output')
    if args.profile == 'rotation' and args.input != 'file':
        parser.error('the rotation profile requires --input file')
    if args.profile == 'idle' and args.records:
        parser.error('idle runs cannot have a fixed record count')
    args.binary = args.binary.resolve()
    try:
        result = run(args)
    except Exception as error:
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps({'status': 'failed', 'error': repr(error),
                                              'seed': args.seed, 'profile': args.profile,
                                              **getattr(error, 'soak_details', {})}) + '\n')
        raise
    result['status'] = 'passed'
    encoded = json.dumps(result, sort_keys=True)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(encoded + '\n')
    print(encoded)


if __name__ == '__main__':
    main()

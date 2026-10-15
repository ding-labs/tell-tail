#!/usr/bin/env python3
"""Cached scans and fixed-workload paired pipelines; no hardware thresholds in CI."""
import argparse
import hashlib
import json
import math
import os
import pathlib
import platform
import random
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
from soak import optional_command, run as soak
from soak_support import Counts, make_batch

ROOT = pathlib.Path(__file__).resolve().parents[1]


def summary(values):
    median = statistics.median(values)
    return {'median': median, 'min': min(values), 'max': max(values),
            'relative_range': (max(values) - min(values)) / median if median else None}


def fixtures(seed):
    for name, line in [('text200', b'INFO ' + b'x' * 194 + b'\n'),
                       ('json500', b'{"level":"INFO","message":"' + b'x' * 470 + b'"}\n'),
                       ('text16k', b'WARN ' + b'x' * 16378 + b'\n')]:
        counts = Counts(bytes_read=len(line))
        counts.levels['warn' if name == 'text16k' else 'info'] = 1
        yield name, line, counts, 1024**2
    payload, counts, _ = make_batch(seed, 'mixed', 1024)
    yield 'mixed', payload, counts, 1024


def cached(args):
    results = []
    for name, payload, batch, limit in fixtures(args.seed):
        copies = min(args.records // batch.records, args.disk_limit_mib * 1024**2 // len(payload))
        assert copies > 0, 'fixture budget is too small'
        expected = Counts()
        for key in expected.__dict__:
            if key == 'levels':
                expected.levels = {key: value * copies for key, value in batch.levels.items()}
            else:
                setattr(expected, key, getattr(batch, key) * copies)
        digest = hashlib.sha256()
        with tempfile.TemporaryDirectory(prefix='tell-tail-bench-') as directory:
            path = pathlib.Path(directory) / 'fixture.log'
            with path.open('wb') as stream:
                remaining = copies
                block_copies = max(1, 1024**2 // len(payload))
                while remaining:
                    block = payload * min(block_copies, remaining)
                    stream.write(block)
                    digest.update(block)
                    remaining -= min(block_copies, remaining)
            trials = []
            for trial in range(args.repeats + 1):
                cpu_before = resource.getrusage(resource.RUSAGE_CHILDREN)
                start = time.perf_counter()
                output = subprocess.check_output([str(args.binary), str(path), '--once', '--output', 'json',
                                                  '--max-record-bytes', str(limit)], timeout=120)
                elapsed = time.perf_counter() - start
                cpu_after = resource.getrusage(resource.RUSAGE_CHILDREN)
                report = json.loads(output.splitlines()[-1])
                expected.verify(report)
                if trial:  # excluded cache warmup
                    trials.append({'seconds': elapsed, 'records_per_second': expected.records / elapsed,
                                   'waited_child_cpu_seconds': cpu_after.ru_utime + cpu_after.ru_stime
                                       - cpu_before.ru_utime - cpu_before.ru_stime})
            result = {'fixture': name, 'fixture_sha256': digest.hexdigest(), 'records': expected.records,
                      'bytes': expected.bytes_read, 'trials': trials,
                      'records_per_second': summary([t['records_per_second'] for t in trials])}
            results.append(result)
            print(f"cached {name}: {result['records_per_second']}", file=sys.stderr, flush=True)
    return results


def stream_args(args, output, **overrides):
    values = dict(binary=args.binary, seconds=args.seconds, records=0, input='stdin', output=output,
                  profile=args.profile, seed=args.seed, rate=0, max_record_bytes=1024, window=60,
                  rss_limit_mib=64, disk_limit_mib=256, file_gib_per_day=10, rotate_bytes=8*1024**2)
    values.update(overrides)
    return SimpleNamespace(**values)


def streaming(args):
    # Warm each output mode, then use the same fixed workload in every measured run.
    warmups = [soak(stream_args(args, mode)) for mode in ('json', 'tui')]
    rate = max(r['records'] / r['production_seconds'] for r in warmups)
    records = max(100, math.ceil(rate * args.seconds / 100) * 100)
    rng = random.Random(args.seed)
    pairs = []
    for index in range(args.pairs):
        order = ['json', 'tui']
        rng.shuffle(order)
        pair = {'order': order, 'runs': {}}
        for mode in order:
            pair['runs'][mode] = soak(stream_args(args, mode, records=records, seconds=args.seconds * 4 + 10))
        headless, tui = (pair['runs'][mode] for mode in ('json', 'tui'))
        assert headless['fixture_sha256'] == tui['fixture_sha256']
        assert headless['expected'] == tui['expected']
        pair['throughput_loss_fraction'] = 1 - headless['seconds'] / tui['seconds']
        pairs.append(pair)
        print(f"pair {index + 1}: TUI loss {pair['throughput_loss_fraction']:.2%}", file=sys.stderr, flush=True)
    losses = [p['throughput_loss_fraction'] for p in pairs]
    variation = {mode: summary([p['runs'][mode]['records_per_second'] for p in pairs]) for mode in ('json', 'tui')}
    generator_warning = any(r['producer_saturation_warning'] for p in pairs for r in p['runs'].values())
    noise_warning = any(v['relative_range'] > .1 for v in variation.values())
    return {'fixed_records_per_run': records, 'warmups': warmups, 'pairs': pairs,
            'throughput': variation, 'median_tui_loss_fraction': statistics.median(losses),
            'possible_generator_limit': generator_warning, 'timing_variation_warning': noise_warning,
            'measurement_qualifies_for_gate': args.pairs >= 5 and args.seconds >= 5
                and not generator_warning and not noise_warning,
            'idle': [soak(stream_args(args, mode, profile='idle')) for mode in ('json', 'tui')],
            'followed_file': soak(stream_args(args, 'prometheus', input='file', rate=1000))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=['cached', 'stream', 'all'], default='all')
    parser.add_argument('--records', type=int, default=1_000_000)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--pairs', type=int, default=5)
    parser.add_argument('--seconds', type=float, default=5, help='target duration of each paired run')
    parser.add_argument('--disk-limit-mib', type=int, default=512)
    parser.add_argument('--seed', type=int, default=1)
    parser.add_argument('--profile', choices=['sustained', 'mixed'], default='sustained')
    parser.add_argument('--binary', type=pathlib.Path, default=ROOT / 'target/release/tell-tail')
    parser.add_argument('--report', type=pathlib.Path)
    args = parser.parse_args()
    if args.records < 100 or args.repeats < 1 or args.pairs < 1 or not 0 < args.seconds <= 600 or args.disk_limit_mib < 1:
        parser.error('invalid workload or measurement budget')
    args.binary = args.binary.resolve()
    result = {'platform': platform.platform(), 'architecture': platform.machine(), 'host': platform.node(),
              'logical_cpus': os.cpu_count(), 'cpu_model': optional_command(['sysctl', '-n', 'machdep.cpu.brand_string'])
                  if sys.platform == 'darwin' else optional_command(['uname', '-m']),
              'commit': optional_command(['git', 'rev-parse', 'HEAD']),
              'workspace_toolchain': optional_command(['rustc', '--version']),
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'configuration': {key: str(value) if isinstance(value, pathlib.Path) else value
                                for key, value in vars(args).items()}}
    if args.suite in ('cached', 'all'):
        result['cached'] = cached(args)
    if args.suite in ('stream', 'all'):
        result['streaming'] = streaming(args)
    encoded = json.dumps(result, sort_keys=True)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(encoded + '\n')
    print(encoded)


if __name__ == '__main__':
    main()

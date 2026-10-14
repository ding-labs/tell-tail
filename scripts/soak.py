#!/usr/bin/env python3
"""Sustained pipe load with exact-count and RSS checks; optional real TUI."""
import argparse
import fcntl
import json
import os
import pathlib
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument('--seconds', type=float, default=60)
parser.add_argument('--tui', action='store_true')
args = parser.parse_args()
assert args.seconds > 0
root = pathlib.Path(__file__).resolve().parents[1]
binary = root / 'target/release/tell-tail'
# 1,000 fixed-size records per batch: 90% info, 9% warn, 1% error.
batch = b''.join(level + b' ' + b'x' * (198 - len(level)) + b'\n'
                 for level in [b'INFO'] * 900 + [b'WARN'] * 90 + [b'ERROR'] * 10)
assert len(batch) == 200_000
with tempfile.TemporaryFile() as reports:
    master = slave = None
    if args.tui:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 26, 100, 0, 0))
    def setup():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([str(binary), '--output', 'tui' if args.tui else 'json'],
                             stdin=subprocess.PIPE, stdout=slave if args.tui else reports,
                             stderr=reports, preexec_fn=setup if args.tui else None,
                             env={**os.environ, 'TERM': 'xterm-256color'})
    batches = [0]
    errors = []
    started = time.monotonic()
    def produce():
        try:
            while time.monotonic() - started < args.seconds:
                child.stdin.write(batch)
                batches[0] += 1
            child.stdin.close()
        except Exception as error:
            errors.append(error)
    producer = threading.Thread(target=produce, daemon=True)
    producer.start()
    rss = []
    next_sample = started + 1
    # Keep only a bounded terminal tail for the final report.
    terminal_tail = bytearray()
    def drain():
        if master is None:
            time.sleep(0.02)
            return
        if select.select([master], [], [], 0.02)[0]:
            try:
                chunk = os.read(master, 65536)
                if b'\x1b[6n' in chunk:
                    os.write(master, b'\x1b[1;1R')
                terminal_tail.extend(chunk)
                del terminal_tail[:-32768]
            except OSError:
                pass
    try:
        while child.poll() is None:
            drain()
            now = time.monotonic()
            if now >= next_sample:
                result = subprocess.run(['ps', '-o', 'rss=', '-p', str(child.pid)], capture_output=True, text=True)
                if result.stdout.strip():
                    rss.append(int(result.stdout.strip()))
                next_sample = now + 1
            if now - started > args.seconds + 10:
                raise AssertionError('load test timed out')
        for _ in range(5):
            drain()
        producer.join(timeout=2)
        assert not producer.is_alive() and not errors, errors
        assert child.returncode == 0, child.returncode
        expected = batches[0] * 1000
        if args.tui:
            final = bytes(terminal_tail).split(b'final source=stdin')[-1]
            for name, expected_value in [('records', expected), ('info', batches[0] * 900), ('warn', batches[0] * 90), ('error', batches[0] * 10)]:
                assert re.search(rb'\b' + name.encode() + b'=' + str(expected_value).encode() + rb'(?: |\r|\n)', final), final
            elapsed = float(re.search(rb'elapsed=([0-9.]+)s', final)[1])
        else:
            reports.seek(0)
            report = json.loads(reports.read().splitlines()[-1])
            assert report['totals']['records'] == expected
            assert report['totals']['levels']['info'] == batches[0] * 900
            assert report['totals']['levels']['warn'] == batches[0] * 90
            assert report['totals']['levels']['error'] == batches[0] * 10
            assert report['bytes_read'] == batches[0] * len(batch)
            elapsed = report['elapsed_seconds']
        peak = max(rss, default=0) / 1024
        assert peak < 64, f'RSS target exceeded: {peak:.1f} MiB'
        print(json.dumps({'mode': 'tui' if args.tui else 'json', 'records': expected,
                          'seconds': elapsed, 'records_per_second': round(expected / elapsed),
                          'peak_sampled_rss_mib': round(peak, 2), 'rss_samples_kib': rss}))
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        if master is not None:
            os.close(master)
            os.close(slave)

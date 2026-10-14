#!/usr/bin/env python3
"""Cached-file scan benchmark. Run after cargo build --release."""
import argparse
import json
import pathlib
import platform
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--records', type=int, default=1_000_000)
parser.add_argument('--repeats', type=int, default=3)
args = parser.parse_args()
assert args.records > 0 and args.repeats > 0
root = pathlib.Path(__file__).resolve().parents[1]
binary = root / 'target/release/tell-tail'
print(platform.platform(), platform.machine(), flush=True)
print(subprocess.check_output(['rustc', '--version'], cwd=root, text=True).strip())
for name, line in [('text', b'INFO ' + b'x' * 194 + b'\n'),
                   ('json', b'{"level":"INFO","message":"' + b'x' * 470 + b'"}\n')]:
    with tempfile.TemporaryDirectory(prefix='tell-tail-bench-') as directory:
        path = pathlib.Path(directory) / 'fixture.log'
        with path.open('wb') as f:
            batch = line * 10000
            for _ in range(args.records // 10000):
                f.write(batch)
            f.write(line * (args.records % 10000))
        rates = []
        for trial in range(args.repeats + 1):
            start = time.perf_counter()
            out = subprocess.check_output([str(binary), str(path), '--once', '--output', 'json'])
            elapsed = time.perf_counter() - start
            report = json.loads(out.splitlines()[-1])
            assert report['totals']['records'] == args.records
            assert report['totals']['levels']['info'] == args.records
            assert report['bytes_read'] == len(line) * args.records
            if trial:  # first pass warms the file cache
                rates.append(args.records / elapsed)
        rates.sort()
        median = rates[len(rates) // 2]
        print(f'{name}: {len(line)} bytes/record; median {median:,.0f} records/s; '
              f'{median * len(line) / 1_000_000:,.1f} MB/s; '
              f'range {rates[0]:,.0f}..{rates[-1]:,.0f}', flush=True)

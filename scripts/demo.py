#!/usr/bin/env python3
"""Generate a repeatable stream with changing warning/error ratios."""
import argparse
import json
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument('--rate', type=int, default=10000, help='approximate records per second')
parser.add_argument('--seconds', type=float, default=0, help='zero runs until interrupted')
args = parser.parse_args()
if args.rate < 1 or args.seconds < 0:
    parser.error('rate must be positive and seconds must be nonnegative')
records = {level: (json.dumps({'level': level, 'message': message}) + '\n').encode()
           for level, message in [('INFO', 'request completed'), ('WARN', 'upstream latency rising'),
                                   ('ERROR', 'upstream timeout')]}
started = time.monotonic()
count = 0
batch_size = min(1000, max(1, args.rate // 20))
try:
    while args.seconds == 0 or time.monotonic() - started < args.seconds:
        phase = int(time.monotonic() - started) % 20
        warning = 5 if phase < 10 else 20
        error = 1 if phase < 15 else 10
        batch = bytearray()
        for _ in range(batch_size):
            slot = count % 100
            level = 'ERROR' if slot < error else 'WARN' if slot < error + warning else 'INFO'
            batch.extend(records[level])
            count += 1
        sys.stdout.buffer.write(batch)
        sys.stdout.buffer.flush()
        time.sleep(max(0, started + count / args.rate - time.monotonic()))
except (BrokenPipeError, KeyboardInterrupt):
    # Avoid a second BrokenPipeError from Python's shutdown flush.
    import os
    os._exit(0)

#!/usr/bin/env python3
"""Measure write-to-visible-count latency in a draining terminal emulator."""
import argparse
import fcntl
import json
import math
import os
import pathlib
import pty
import re
import select
import signal
import struct
import subprocess
import termios
import time
import pyte

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=pathlib.Path, default=ROOT / 'target/debug/tell-tail')
parser.add_argument('--samples', type=int, default=25)
args = parser.parse_args()
assert 1 <= args.samples <= 1000
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 26, 100, 0, 0))
screen = pyte.Screen(100, 26)
stream = pyte.ByteStream(screen)


def setup():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)


child = subprocess.Popen([str(args.binary), '--output', 'tui'], stdin=subprocess.PIPE,
                         stdout=slave, stderr=subprocess.PIPE, preexec_fn=setup,
                         env={**os.environ, 'TERM': 'xterm-256color'})


def drain(seconds):
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        if select.select([master], [], [], min(.01, max(0, until - time.monotonic())))[0]:
            try:
                data = os.read(master, 65536)
            except OSError:
                return
            if not data:
                return
            stream.feed(data)


def text():
    return '\n'.join(screen.display)


def until(predicate):
    deadline = time.monotonic() + 3
    while not predicate(text()):
        assert time.monotonic() < deadline and child.poll() is None, text()
        drain(.01)


def count_is(count):
    return lambda text: re.search(rf'\b{count} records\b', text) is not None


try:
    until(lambda text: 'tell-tail' in text)
    latencies = []
    for count in range(1, args.samples + 1):
        began = time.monotonic()
        child.stdin.write(b'INFO freshness\n')
        child.stdin.flush()
        until(count_is(count))
        latencies.append(time.monotonic() - began)
    os.write(master, b' ')
    until(lambda text: 'DISPLAY FROZEN' in text)
    child.stdin.write(b'WARN while frozen\n')
    child.stdin.flush()
    drain(.35)
    assert count_is(args.samples)(text()), text()
    os.write(master, b' ')
    until(count_is(args.samples + 1))
    os.write(master, b'?')
    until(lambda text: 'ingestion continues' in text)
    os.write(master, b'\x1b')
    until(lambda text: 'Parse failures:' in text)
    screen.resize(lines=10, columns=40)
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 10, 40, 0, 0))
    child.send_signal(signal.SIGWINCH)
    until(lambda text: 'Resize' in text)
    child.stdin.write(b'{bad}\n')
    child.stdin.flush()
    until(lambda text: 'Unknown: 1' in text and 'Parse failures: 1' in text)
    began = time.monotonic()
    os.write(master, b'q')
    while child.poll() is None and time.monotonic() - began < 2:
        drain(.01)
    shutdown = time.monotonic() - began
    assert child.poll() == 0, (child.poll(), text())
    assert shutdown < 1, shutdown
    p95 = sorted(latencies)[math.ceil(.95 * len(latencies)) - 1]
    print(json.dumps({'samples': len(latencies), 'p95_seconds': p95,
                      'max_seconds': max(latencies), 'shutdown_seconds': shutdown,
                      'input': 'one record per visible acknowledgment', 'geometry': '100x26'}))
    assert p95 <= .5, f'dashboard freshness gate failed: {p95:.3f}s'
finally:
    if child.poll() is None:
        child.kill()
        child.wait(timeout=3)
    child.stdin.close()
    child.stderr.close()
    os.close(master)
    os.close(slave)

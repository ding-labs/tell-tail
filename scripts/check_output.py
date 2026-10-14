#!/usr/bin/env python3
"""Inherited flags, blocked-writer teardown, and helper lifetime regressions."""
import argparse
import fcntl
import json
import os
import pathlib
import signal
import socket
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=pathlib.Path,
                    default=pathlib.Path(__file__).resolve().parents[1] / 'target/debug/tell-tail')
args = parser.parse_args()
# Darwin may expose additional kernel-managed bits after the first write.
# Compare the caller-controlled status flags, including the original regression.
STATUS_FLAGS = os.O_NONBLOCK | os.O_APPEND | getattr(os, 'O_ASYNC', 0)


def children(pid):
    result = subprocess.run(['pgrep', '-P', str(pid)], capture_output=True, text=True)
    assert result.returncode in (0, 1), result.stderr
    return [int(value) for value in result.stdout.split()]


def stopped(pid):
    result = subprocess.run(['ps', '-o', 'stat=', '-p', str(pid)], capture_output=True, text=True)
    return not result.stdout.strip() or result.stdout.strip().startswith('Z')


for action in ['term', 'kill', 'deadline', 'broken-pipe']:
    reader, writer = socket.socketpair()
    writer.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024)
    initial = (fcntl.fcntl(writer, fcntl.F_GETFL) & STATUS_FLAGS)
    child = subprocess.Popen([str(args.binary), '--output', 'json', '--interval-ms', '100'],
                             stdin=subprocess.PIPE, stdout=writer, stderr=subprocess.PIPE)
    helpers = []
    try:
        # Observe a complete report rather than sleeping to guess initialization.
        reader.settimeout(3)
        report = bytearray()
        while b'\n' not in report:
            report.extend(reader.recv(4096))
        json.loads(report.splitlines()[0])
        helpers = children(child.pid)
        assert len(helpers) == 1, helpers
        assert (fcntl.fcntl(writer, fcntl.F_GETFL) & STATUS_FLAGS) == initial, 'inherited flags changed'
        if action == 'broken-pipe':
            reader.close()
            expected = 0
        else:
            time.sleep(.4)  # unread socket fills; no application readiness assumption
            if action == 'term':
                child.send_signal(signal.SIGTERM)
                expected = 1
            elif action == 'kill':
                child.kill()
                expected = -signal.SIGKILL
            else:
                expected = 1
        start = time.monotonic()
        child.wait(timeout=3 if action == 'deadline' else 1)
        assert child.returncode == expected, (action, child.returncode, child.stderr.read())
        if action == 'deadline':
            assert b'exceeded two seconds' in child.stderr.read()
        assert (fcntl.fcntl(writer, fcntl.F_GETFL) & STATUS_FLAGS) == initial
        until = time.monotonic() + 1
        while not all(stopped(pid) for pid in helpers) and time.monotonic() < until:
            time.sleep(.01)
        assert all(stopped(pid) for pid in helpers), 'output helper survived parent'
        print(f'{action}: flags preserved, helper stopped, exit in {time.monotonic() - start:.3f}s')
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        child.stdin.close()
        child.stderr.close()
        reader.close()
        writer.close()

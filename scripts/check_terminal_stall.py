#!/usr/bin/env python3
"""A full PTY must not prevent cancellation or termios restoration."""
import fcntl
import json
import os
import pathlib
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = pathlib.Path(__file__).resolve().parents[1] / 'target/debug/tell-tail'
master, slave = pty.openpty()
# A large first frame ensures output blocks after the initial terminal query.
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 300, 300, 0, 0))
def setup():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
supervisor = '''
import json, signal, subprocess, sys, termios
with open('/dev/tty', 'rb', buffering=0) as tty:
    before = termios.tcgetattr(tty)
    child = subprocess.Popen([sys.argv[1], '--output', 'tui'])
    signal.signal(signal.SIGTERM, lambda *_: child.send_signal(signal.SIGTERM))
    code = child.wait()
    after = termios.tcgetattr(tty)
    before[3] &= ~getattr(termios, 'PENDIN', 0)
    after[3] &= ~getattr(termios, 'PENDIN', 0)
    with open(sys.argv[2], 'w') as report:
        json.dump({'code': code, 'restored': before == after}, report)
'''
with tempfile.TemporaryDirectory() as directory:
    report = pathlib.Path(directory) / 'report.json'
    child = subprocess.Popen([sys.executable, '-c', supervisor, str(binary), str(report)],
                             stdin=subprocess.PIPE, stdout=slave, stderr=slave,
                             preexec_fn=setup, env={**os.environ, 'TERM': 'xterm-256color'})
    try:
        output = bytearray()
        until = time.monotonic() + 2
        while time.monotonic() < until:
            if select.select([master], [], [], .05)[0]:
                output.extend(os.read(master, 65536))
                if b'\x1b[6n' in output:
                    os.write(master, b'\x1b[1;1R')
                if b'\x1b[?1049h' in output:
                    break
        assert b'\x1b[?1049h' in output, output
        # Deliberately stop consuming terminal output and let the first frame fill it.
        time.sleep(.3)
        began = time.monotonic()
        child.send_signal(signal.SIGTERM)
        # The supervisor keeps the controlling session alive for tcgetattr.
        # Its own final tty close may drain on Darwin; time the application exit
        # and restoration acknowledgment, not that unrelated supervisor close.
        while not report.exists() and time.monotonic() - began < 1:
            time.sleep(.005)
        assert report.exists(), 'application shutdown exceeded one second'
        result = json.loads(report.read_text())
        assert result['code'] == 1, result
        assert result['restored'], result
        print(f'stalled PTY: terminal restored and process stopped in {time.monotonic() - began:.3f}s')
        until = time.monotonic() + 2
        while child.poll() is None and time.monotonic() < until:
            if select.select([master], [], [], .01)[0]:
                os.read(master, 65536)
        child.wait(timeout=1)
    finally:
        # Release a full PTY before reaping a failed test on Darwin.
        os.close(master)
        if child.poll() is None:
            child.kill()
            child.wait(timeout=3)
        child.stdin.close()
        os.close(slave)

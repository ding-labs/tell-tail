#!/usr/bin/env python3
"""Exercise piped logs, keyboard input, resizing, and restoration through a PTY."""
import fcntl
import http.server
import json
import os
import pathlib
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

root = pathlib.Path(__file__).resolve().parents[1]
binary = root / 'target/debug/tell-tail'
alerts = '--alerts' in sys.argv[1:]
received = []
class Receiver(http.server.BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(2)
    def do_POST(self):
        length = int(self.headers['Content-Length'])
        assert length < 4096
        received.append(json.loads(self.rfile.read(length)))
        self.send_response(204)
        self.end_headers()
    def log_message(self, *args):
        pass

server = http.server.HTTPServer(('127.0.0.1', 0), Receiver) if alerts else None
alert_args = (['--alert-webhook', f'http://127.0.0.1:{server.server_port}/',
               '--alert-error-percent', '25', '--alert-min-records', '2'] if alerts else [])
for action, expected in [('q', 0), ('signal', 143), ('interrupt', 130), ('eof', 0)]:
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 26, 100, 0, 0))
    def setup():
        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    # Keep the session leader alive while checking termios: Darwin revokes the
    # slave when the controlling session exits.
    supervisor = """
import os, signal, subprocess, sys, termios
with open('/dev/tty', 'rb', buffering=0) as tty:
    before = termios.tcgetattr(tty)
    child = subprocess.Popen([sys.argv[1], '--output', 'tui', *sys.argv[2:]])
    signal.signal(signal.SIGTERM, lambda sig, frame: child.send_signal(sig))
    signal.signal(signal.SIGINT, lambda sig, frame: child.send_signal(sig))
    code = child.wait()
    after = termios.tcgetattr(tty)
    # Darwin may set PENDIN while switching canonical mode; it is transient state.
    before[3] &= ~getattr(termios, 'PENDIN', 0)
    after[3] &= ~getattr(termios, 'PENDIN', 0)
    print('TERMINAL_RESTORED' if after == before else repr(('TERMINAL_CHANGED', before, after)), flush=True)
    sys.exit(code)
"""
    child = subprocess.Popen([sys.executable, '-c', supervisor, str(binary), *alert_args], stdin=subprocess.PIPE,
                             stdout=slave, stderr=slave, preexec_fn=setup,
                             env={**os.environ, 'TERM': 'xterm-256color'})
    output = bytearray()
    def drain(seconds):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            ready, _, _ = select.select([master], [], [], min(0.05, max(0, until - time.monotonic())))
            if ready:
                try:
                    chunk = os.read(master, 65536)
                    output.extend(chunk)
                    # Answer the cursor-position query as a terminal emulator would.
                    if b'\x1b[6n' in chunk:
                        os.write(master, b'\x1b[1;1R')
                except OSError:
                    break
    try:
        child.stdin.write(b'INFO first\n')
        child.stdin.flush()
        ready_by = time.monotonic() + 3
        while b'tell-tail' not in output and time.monotonic() < ready_by:
            drain(0.05)
        assert b'tell-tail' in output, output.decode(errors='replace')
        os.write(master, b' ')
        drain(0.3)
        assert b'DISPLAY' in output and b'FROZEN' in output
        child.stdin.write(b'ERROR second\n' if alerts else b'WARN second\n')
        child.stdin.flush()
        if server:
            deadline = time.monotonic() + 3
            while not select.select([server], [], [], 0)[0]:
                assert time.monotonic() < deadline, 'no alert while display frozen'
                drain(.05)
            server.handle_request()
            assert received[-1]['error_percent'] == 50
            assert received[-1]['records'] == 2
        os.write(master, b'?')
        drain(0.3)
        assert b'ingestion' in output and b'continues' in output
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 10, 40, 0, 0))
        child.send_signal(signal.SIGWINCH)
        drain(0.3)
        assert b'Resize' in output
        began = time.monotonic()
        if action == 'q':
            os.write(master, b'q')
        elif action == 'signal':
            child.send_signal(signal.SIGTERM)
        elif action == 'interrupt':
            child.send_signal(signal.SIGINT)
        else:
            child.stdin.close()
        while child.poll() is None and time.monotonic() - began < 3:
            drain(0.05)
        shutdown = time.monotonic() - began
        drain(0.1)
        assert child.poll() == expected, (child.poll(), output.decode(errors='replace'))
        assert shutdown < 1, shutdown
        assert b'final source=stdin' in output and b'records=2 ' in output
        assert b'TERMINAL_RESTORED' in output, output.decode(errors='replace')
        assert b'\x1b[?1049l' in output, 'alternate screen not restored'
        print(f'{action}: counts, keys, resize, terminal restoration; shutdown {shutdown:.3f}s')
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
        if not child.stdin.closed:
            child.stdin.close()
        os.close(master)
        os.close(slave)
if server:
    server.server_close()
    assert len(received) == 4, received

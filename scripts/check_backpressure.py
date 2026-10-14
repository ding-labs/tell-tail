#!/usr/bin/env python3
"""A stalled output socket must not prevent cancellation or grow a report queue."""
import pathlib
import signal
import socket
import subprocess
import time

binary = pathlib.Path(__file__).resolve().parents[1] / 'target/debug/tell-tail'
reader, writer = socket.socketpair()
writer.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024)
child = subprocess.Popen([str(binary), '--output', 'json', '--interval-ms', '100'], stdin=subprocess.PIPE,
                         stdout=writer, stderr=subprocess.PIPE)
writer.close()
try:
    child.stdin.write(b'INFO event\n')
    child.stdin.flush()
    # Let the deliberately unread socket fill.
    time.sleep(1)
    began = time.monotonic()
    child.send_signal(signal.SIGTERM)
    code = child.wait(timeout=1)
    assert code in (1, 143), code
    stderr = child.stderr.read().decode()
    if code == 1:
        assert 'output blocked during shutdown' in stderr, stderr
    print(f'stalled output exits in {time.monotonic() - began:.3f}s (code {code})')
finally:
    if child.poll() is None:
        child.kill()
        child.wait()
    child.stdin.close()
    child.stderr.close()
    reader.close()

#!/usr/bin/env python3
"""A publication I/O failure must preserve the preceding complete metrics file."""
import os
import pathlib
import subprocess
import tempfile
import time

binary = pathlib.Path(__file__).resolve().parents[1] / 'target/debug/tell-tail'
with tempfile.TemporaryDirectory(prefix='tell-tail-metrics-failure-') as directory:
    directory = pathlib.Path(directory)
    source = directory / 'input.log'
    metrics = directory / 'metrics.prom'
    source.write_text('WARN retained\n')
    source.chmod(0o644)
    directory.chmod(0o755)
    # Container test runners may be root; exercise directory permissions as an
    # unprivileged child instead of silently skipping the failure path.
    privileged = os.geteuid() == 0
    if privileged:
        os.chown(directory, 65534, 65534)
    def unprivileged():
        if privileged:
            os.setgroups([])
            os.setgid(65534)
            os.setuid(65534)
    child = subprocess.Popen([str(binary), str(source), '--from', 'start', '--output', 'prometheus',
                              '--metrics-file', str(metrics), '--interval-ms', '100'],
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=unprivileged)
    try:
        deadline = time.monotonic() + 5
        while not metrics.exists():
            assert child.poll() is None and time.monotonic() < deadline, 'metrics were not published'
            time.sleep(.01)
        directory.chmod(0o555)
        previous = metrics.read_bytes()
        output, error = child.communicate(timeout=3)
        assert child.returncode == 1, (child.returncode, error)
        assert output == b'' and error
        assert metrics.read_bytes() == previous
        assert previous.endswith(b'\n') and previous.count(b'tell_tail_records_total{level=') == 7
        print('Publication failure preserves the preceding complete metrics document')
    finally:
        directory.chmod(0o755)
        if child.poll() is None:
            child.kill()
            child.wait(timeout=3)

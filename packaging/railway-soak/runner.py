#!/usr/bin/env python3
"""Run a frozen Linux candidate once; retain synthetic evidence for collection."""
import hashlib
import io
import json
import os
import pathlib
import signal
import subprocess
import sys
import tarfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

SOURCE = pathlib.Path('/source')
BINARY = pathlib.Path('/candidate/tell-tail')
RESULTS = pathlib.Path('/results')
IDENTITY = pathlib.Path('/candidate/identity.json')
PHASES = [('stdin-ndjson', ['--profile', 'mixed']),
          ('file-prometheus', ['--input', 'file', '--profile', 'rotation',
                               '--output', 'prometheus'])]
JOURNAL = RESULTS / 'campaign.json'
CHILD = None
STOP = threading.Event()


def atomic_json(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def git(*args):
    return subprocess.check_output(['git', '-C', str(SOURCE), *args], text=True).strip()


def verify_identity():
    identity = json.loads(IDENTITY.read_text())
    assert git('rev-parse', 'HEAD') == identity['source_commit'], 'source changed'
    assert not git('status', '--porcelain'), 'source checkout is dirty'
    assert hashlib.sha256(BINARY.read_bytes()).hexdigest() == identity['binary_sha256'], 'binary changed'
    build = json.loads(pathlib.Path('/candidate/BUILD.json').read_text())
    assert build['source_commit'] == identity['source_commit'] and build['dirty'] is False
    assert build['target'] == 'x86_64-unknown-linux-musl'
    return identity


def review(report, identity, seconds):
    assert report['status'] == 'passed', report
    assert report['commit'] == identity['source_commit'] and report['workspace_dirty'] is False
    assert report['harness_commit'] == identity['source_commit'] and report['harness_dirty'] is False
    assert report['binary_sha256'] == identity['binary_sha256']
    assert report['configuration']['seconds'] == seconds
    assert report['production_seconds'] >= seconds
    assert report['max_observation_gap_seconds'] <= 5
    assert report['peak_sampled_rss_mib'] < 64
    assert report['producer_saturation_warning'] is False
    assert report['resource_samples'] > 0
    assert report['snapshot_age_p95_upper_ms'] is not None
    assert report['snapshot_age_p95_upper_ms'] <= 1000
    if seconds == 86400:
        medians = report['rss_10minute_medians_mib']
        assert len(medians) >= 142, 'missing full-day memory blocks'
        assert all(m <= medians[1] + max(4, medians[1] * .2) for m in medians[2:])


def run_phase(name, extra, seconds, identity):
    global CHILD
    assert not STOP.is_set(), 'campaign stopping'
    verify_identity()
    report = RESULTS / (name + '.json')
    command = [sys.executable, str(SOURCE / 'scripts/soak.py'), '--binary', str(BINARY),
               '--source-directory', str(SOURCE), '--seconds', str(seconds),
               '--report', str(report), *extra]
    with (RESULTS / (name + '.stdout')).open('w') as output, (RESULTS / (name + '.progress')).open('w') as progress:
        CHILD = subprocess.Popen(command, stdout=output, stderr=subprocess.PIPE, text=True,
                                 start_new_session=True)
        def drain():
            for line in CHILD.stderr:
                progress.write(line)
                progress.flush()
                print(json.dumps({'phase': name, 'progress': line.strip()}), flush=True)
        reader = threading.Thread(target=drain, daemon=True)
        reader.start()
        try:
            code = CHILD.wait(timeout=seconds + 60)
        except subprocess.TimeoutExpired:
            os.killpg(CHILD.pid, signal.SIGKILL)
            CHILD.wait()
            raise RuntimeError(f'{name} exceeded wall deadline')
        reader.join(timeout=5)
        assert not reader.is_alive(), 'progress drain failed'
        assert code == 0, f'{name} exited {code}'
    verify_identity()
    reviewed = json.loads(report.read_text())
    review(reviewed, identity, seconds)
    return {'phase': name, 'status': 'passed', 'report': report.name,
            'sha256': hashlib.sha256(report.read_bytes()).hexdigest(),
            'production_seconds': reviewed['production_seconds']}


def campaign():
    RESULTS.mkdir(parents=True, exist_ok=True)
    if JOURNAL.exists():
        state = json.loads(JOURNAL.read_text())
        if state['status'] not in ('passed', 'failed', 'interrupted'):
            state.update(status='interrupted', error='container restarted; partial duration invalid',
                         finished_at=time.time())
            atomic_json(JOURNAL, state)
        print(json.dumps({'event': 'existing_campaign_not_restarted', 'state': state}), flush=True)
        return
    state = {'schema_version': 1, 'status': 'preflight', 'started_at': time.time(),
             'phases': [], 'preflight': [], 'deployment_id': os.environ.get('RAILWAY_DEPLOYMENT_ID')}
    atomic_json(JOURNAL, state)
    try:
        identity = verify_identity()
        state['identity'] = identity
        atomic_json(RESULTS / 'identity.json', identity)
        atomic_json(RESULTS / 'runtime.json', {
            'platform': __import__('platform').platform(),
            'cpu_max': read_optional('/sys/fs/cgroup/cpu.max'),
            'memory_max': read_optional('/sys/fs/cgroup/memory.max'),
            'memory_swap_max': read_optional('/sys/fs/cgroup/memory.swap.max'),
            'runner_sha256': hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()})
        for name, args in PHASES:
            state['preflight'].append(run_phase('preflight-' + name, args, 3, identity))
            atomic_json(JOURNAL, state)
        state.update(status='running', production_started_at=time.time())
        for name, args in PHASES:
            state.update(phase=name, phase_started_at=time.time())
            atomic_json(JOURNAL, state)
            print(json.dumps({'event': 'phase_started', 'state': state}), flush=True)
            state['phases'].append(run_phase(name, args, 86400, identity))
            atomic_json(JOURNAL, state)
            print(json.dumps({'event': 'phase_passed', 'phase': state['phases'][-1]}), flush=True)
        state['status'] = 'passed'
    except BaseException as error:
        state.update(status='interrupted' if STOP.is_set() else 'failed', error=repr(error))
    finally:
        state['finished_at'] = time.time()
        atomic_json(JOURNAL, state)
        print(json.dumps({'event': 'campaign_finished', 'state': state}), flush=True)


def read_optional(path):
    p = pathlib.Path(path)
    return p.read_text().strip() if p.exists() else None


class EvidenceHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/health':
            payload, kind = b'ok\n', 'text/plain'
        elif self.path == '/campaign.json' and JOURNAL.exists():
            payload, kind = JOURNAL.read_bytes(), 'application/json'
        elif self.path == '/evidence.tar.gz':
            stream = io.BytesIO()
            with tarfile.open(fileobj=stream, mode='w:gz') as archive:
                for path in sorted(RESULTS.iterdir()):
                    if path.is_file() and path.suffix in ('.json', '.stdout', '.progress'):
                        assert path.stat().st_size < 32 * 1024**2, 'unexpected evidence size'
                        archive.add(path, arcname=path.name, recursive=False)
            payload, kind = stream.getvalue(), 'application/gzip'
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(payload)))
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *_):
        pass


def stop(*_):
    STOP.set()
    if CHILD is not None and CHILD.poll() is None:
        os.killpg(CHILD.pid, signal.SIGTERM)


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    server = ThreadingHTTPServer(('0.0.0.0', int(os.environ.get('PORT', '8080'))), EvidenceHandler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    campaign()
    # Keep the small evidence server available until the authorized cleanup collects it.
    STOP.wait()
    server.shutdown()

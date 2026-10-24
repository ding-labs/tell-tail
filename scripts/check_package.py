#!/usr/bin/env python3
"""Verify the Cargo source package and install it in a clean temporary directory."""
import argparse
import hashlib
import http.client
import json
import os
import pathlib
import shutil
import selectors
import signal
import subprocess
import tarfile
import tempfile
import time
import tomllib
import urllib.parse


def check_web_install(binary, directory):
    """Exercise default-feature web output from the extracted, installed crate."""
    child = subprocess.Popen([str(binary), '--output', 'web'], cwd=directory,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        child.stdin.write(b'INFO first\nWARN second\n')
        child.stdin.close()
        diagnostic = bytearray()
        deadline = time.monotonic() + 3
        with selectors.DefaultSelector() as selector:
            selector.register(child.stderr, selectors.EVENT_READ)
            url = None
            while url is None:
                assert time.monotonic() < deadline, 'installed web server did not print its URL'
                if selector.select(.1):
                    part = os.read(child.stderr.fileno(), 4096)
                    assert part, f'installed server exited: {diagnostic!r}'
                    diagnostic.extend(part)
                    assert len(diagnostic) < 16384
                    url = next((word for word in diagnostic.decode().split()
                                if word.startswith('http://') and word.endswith('/')), None)
        location = urllib.parse.urlsplit(url)
        assert location.hostname == '127.0.0.1' and location.port
        while True:
            connection = http.client.HTTPConnection(location.hostname, location.port, timeout=1)
            try:
                connection.request('GET', '/api/snapshot')
                response = connection.getresponse()
                assert response.status == 200 and response.getheader('Cache-Control') == 'no-store'
                payload = response.read(65537)
                assert len(payload) <= 65536
                frame = json.loads(payload)
            finally:
                connection.close()
            if frame['metrics']['state'] == 'complete':
                break
            assert time.monotonic() < deadline, 'installed server did not retain final state'
            time.sleep(.01)
        assert frame['web_schema_version'] == 1 and isinstance(frame['sequence'], str)
        assert frame['metrics']['totals']['records'] == '2'
        assert frame['metrics']['totals']['levels']['warn'] == '1'
        assert 'schema_version' not in frame['metrics']
        for path, mime, marker in [
            ('/', 'text/html', b'Live metrics'),
            ('/dashboard.css', 'text/css', b'.metrics-grid'),
            ('/dashboard.mjs', 'text/javascript', b'new EventSource'),
            ('/frame.mjs', 'text/javascript', b'parseFrame'),
            ('/render.mjs', 'text/javascript', b'textContent'),
            ('/favicon.svg', 'image/svg+xml', b'<svg'),
            ('/logo.svg', 'image/svg+xml', b'<svg'),
        ]:
            connection = http.client.HTTPConnection(location.hostname, location.port, timeout=1)
            try:
                connection.request('GET', path)
                response = connection.getresponse()
                assert response.status == 200, path
                assert response.getheader('Content-Type').startswith(mime), path
                assert response.getheader('Cache-Control') == 'no-store', path
                assert "script-src 'self'" in response.getheader('Content-Security-Policy'), path
                assert marker in response.read(), path
            finally:
                connection.close()
        assert child.poll() is None, 'web output exited at normal EOF'
        child.send_signal(signal.SIGTERM)
        assert child.wait(timeout=1) == 143
        assert child.stdout.read() == b'', 'web output wrote to stdout'
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        child.stdout.close()
        child.stderr.close()

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--allow-dirty', action='store_true', help='local development only; recorded in the result')
args = parser.parse_args()
manifest = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
package_name = f"{manifest['name']}-{manifest['version']}"
command = ['cargo', 'package', '--locked']
if args.allow_dirty:
    command.append('--allow-dirty')
subprocess.run(command, cwd=ROOT, check=True)
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT))
archive = pathlib.Path(metadata['target_directory']) / 'package' / (package_name + '.crate')
with tempfile.TemporaryDirectory(prefix='tell-tail-package-') as directory:
    directory = pathlib.Path(directory)
    with tarfile.open(archive) as tar:
        names = tar.getnames()
        assert all(name.startswith(package_name + '/') and '..' not in pathlib.PurePosixPath(name).parts
                   for name in names), 'unexpected archive paths'
        assert all(member.isfile() or member.isdir() for member in tar.getmembers()), 'unexpected archive links'
        assert not any('/work/' in name or '/fuzz/' in name or '/experiments/' in name
                       or '/browser-tests/' in name or '__pycache__' in name for name in names)
        assert package_name + '/src/web/mod.rs' in names
        assert package_name + '/tests/fixtures/output/empty-web-v1.json' in names
        for asset in ['index.html', 'dashboard.css', 'dashboard.mjs', 'frame.mjs', 'render.mjs', 'logo.svg', 'favicon.svg']:
            assert package_name + '/web/' + asset in names
        # Copy only the validated regular files/directories. This also supports
        # Python 3.11 builds without tarfile's newer extraction-filter API.
        for member in tar.getmembers():
            destination = directory / member.name
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                with tar.extractfile(member) as source_file, destination.open('wb') as output_file:
                    shutil.copyfileobj(source_file, output_file)
                destination.chmod(member.mode & 0o777)
    source = directory / package_name
    install = directory / 'installed'
    subprocess.run(['cargo', 'install', '--path', str(source), '--root', str(install), '--locked', '--offline'],
                   cwd=source, check=True,
                   env={**os.environ, 'CARGO_TARGET_DIR': str(directory / 'build')})
    binary = install / 'bin/tell-tail'
    output = subprocess.check_output([str(binary), '--output', 'json'], cwd=directory,
                                     input=b'INFO first\nWARN second\n{"level":"error"}\n{bad}\n', timeout=5)
    snapshot = json.loads(output.splitlines()[-1])
    assert snapshot['schema_version'] == 2
    assert snapshot['totals'] == {'records': 4, 'parse_failures': 1, 'oversized_records': 0,
                                 'levels': {'trace': 0, 'debug': 0, 'info': 1, 'warn': 1,
                                            'error': 1, 'fatal': 0, 'unknown': 1}}
    assert snapshot['pending_bytes'] == 0 and snapshot['kind'] == 'final'
    check_web_install(binary, directory)
print(json.dumps({'package': package_name, 'files': len(names), 'bytes': archive.stat().st_size,
                  'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
                  'allow_dirty': args.allow_dirty, 'clean_install_smoke': 'passed',
                  'offline_install': True, 'installed_web_smoke': 'passed',
                  'installed_dashboard_assets': 'passed'}))

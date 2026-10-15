#!/usr/bin/env python3
"""Verify the Cargo source package and install it in a clean temporary directory."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

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
        assert not any('/work/' in name or '/fuzz/' in name or '__pycache__' in name for name in names)
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
    subprocess.run(['cargo', 'install', '--path', str(source), '--root', str(install), '--locked'],
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
print(json.dumps({'package': package_name, 'files': len(names), 'bytes': archive.stat().st_size,
                  'sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
                  'allow_dirty': args.allow_dirty, 'clean_install_smoke': 'passed'}))

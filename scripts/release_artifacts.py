#!/usr/bin/env python3
"""Assemble and verify native release archives without publishing anything."""
import argparse
import gzip
import hashlib
import io
import json
import pathlib
import re
import tarfile

from release_gate import VERSION

FORMULA_TARGETS = {
    'aarch64-apple-darwin': 'MAC',
    'x86_64-unknown-linux-musl': 'LINUX_X86',
    'aarch64-unknown-linux-musl': 'LINUX_ARM',
}
TARGETS = tuple(FORMULA_TARGETS)


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def archive_name(version, target):
    if not re.fullmatch(VERSION, version) or target not in TARGETS:
        raise ValueError('unsupported release version or target')
    return f'tell-tail-{version}-{target}.tar.gz'


def write_archive(destination, files, timestamp):
    """Stable archive metadata; this does not promise reproducible compilation."""
    with destination.open('wb') as output:
        with gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=timestamp) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as archive:
                for name, (content, executable) in sorted(files.items()):
                    info = tarfile.TarInfo(name)
                    info.size, info.mtime = len(content), timestamp
                    info.mode = 0o755 if executable else 0o644
                    archive.addfile(info, io.BytesIO(content))


def validate_manifest(manifest, directory):
    if manifest.get('schema_version') != 1:
        raise ValueError('unsupported artifact manifest')
    version = manifest['version']
    if (not re.fullmatch(VERSION, version)
            or not re.fullmatch(r'[0-9a-f]{40}', manifest['source_commit'])
            or type(manifest['dirty']) is not bool):
        raise ValueError('invalid candidate identity')
    assets = manifest['assets']
    if len(assets) != len(TARGETS) or {asset['target'] for asset in assets} != set(TARGETS):
        raise ValueError('one archive for each supported target is required')
    for asset in assets:
        if asset['name'] != archive_name(version, asset['target']):
            raise ValueError('unexpected archive filename')
        path = directory / asset['name']
        if path.stat().st_size != asset['size'] or digest(path) != asset['sha256']:
            raise ValueError(f'archive checksum/size mismatch: {asset["name"]}')


def assemble(directory):
    records = [json.loads(path.read_text()) for path in sorted(directory.glob('manifest-*.json'))]
    if len(records) != len(TARGETS):
        raise ValueError('expected one native build manifest per supported target')
    identity = {key: records[0][key] for key in ('version', 'source_commit', 'dirty')}
    if any(any(record[key] != value for key, value in identity.items()) for record in records):
        raise ValueError('native builds do not describe the same candidate')
    manifest = {'schema_version': 1, **identity, 'assets': [record['asset'] for record in records]}
    validate_manifest(manifest, directory)
    (directory / 'release.json').write_text(json.dumps(manifest, indent=2) + '\n')
    names = sorted([asset['name'] for asset in manifest['assets']] + ['release.json'])
    (directory / 'SHA256SUMS').write_text(''.join(f'{digest(directory / name)}  {name}\n' for name in names))
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=pathlib.Path)
    parser.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    if args.verify:
        manifest = json.loads((args.directory / 'release.json').read_text())
        validate_manifest(manifest, args.directory)
    else:
        manifest = assemble(args.directory)
    print(json.dumps(manifest, indent=2))


if __name__ == '__main__':
    main()

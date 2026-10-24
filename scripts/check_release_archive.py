#!/usr/bin/env python3
"""Extract a native archive safely and exercise the packaged executable."""
import argparse
import json
import pathlib
import subprocess
import tarfile
import tempfile

from check_package import check_web_install


def check(path):
    prefix = path.name.removesuffix('.tar.gz')
    with tempfile.TemporaryDirectory(prefix='tell-tail-archive-') as directory:
        directory = pathlib.Path(directory)
        with tarfile.open(path) as archive:
            for member in archive:
                parts = pathlib.PurePosixPath(member.name).parts
                if (not parts or parts[0] != prefix or '..' in parts or not member.isfile()):
                    raise ValueError('unexpected archive path or entry type')
                destination = directory / member.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(archive.extractfile(member).read())
                destination.chmod(member.mode & 0o777)
        bundle = directory / prefix
        build = json.loads((bundle / 'BUILD.json').read_text())
        if prefix != f'tell-tail-{build["version"]}-{build["target"]}':
            raise ValueError('archive filename does not match its build identity')
        binary = bundle / 'tell-tail'
        version = subprocess.check_output([str(binary), '--version'], text=True, timeout=5).strip()
        if version != f'tell-tail {build["version"]}':
            raise ValueError('packaged binary version mismatch')
        output = subprocess.check_output([str(binary), '--output', 'json'], timeout=5,
                                         input=b'INFO first\nWARN second\nFATAL third\nunknown\n')
        snapshot = json.loads(output)
        if (snapshot['totals']['records'] != 4 or snapshot['error_ratio'] != 0.25
                or snapshot['totals']['levels']['warn'] != 1 or snapshot['kind'] != 'final'):
            raise ValueError('packaged binary fixture failed')
        if not (bundle / 'LICENSE').is_file() or not (bundle / 'THIRD_PARTY_NOTICES.md').is_file():
            raise ValueError('license notices missing')
        if build['target'].endswith('-linux-musl'):
            for name in ('musl-1.2.3.COPYRIGHT', 'musl-1.2.3-SOURCE-NOTICES.txt'):
                if not (bundle / 'THIRD_PARTY_LICENSES/musl' / name).is_file():
                    raise ValueError('static musl license notices missing')
        check_web_install(binary, directory)
        print(json.dumps({'archive': path.name, 'native_smoke': 'passed',
                          'embedded_web_smoke': 'passed', 'build': build}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=pathlib.Path)
    check(parser.parse_args().archive)

#!/usr/bin/env python3
"""Package a tested native binary, licenses and build identity; never publish."""
import argparse
import json
import os
import pathlib
import platform
import subprocess
import tomllib

from release_artifacts import TARGETS, archive_name, digest, write_archive

ROOT = pathlib.Path(__file__).resolve().parents[1]


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def licenses(metadata):
    packages = {package['id']: package for package in metadata['packages']}
    nodes = {node['id']: node for node in metadata['resolve']['nodes']}
    root = metadata['resolve']['root']
    visited, pending = set(), [root]
    while pending:
        node = pending.pop()
        if node in visited:
            continue
        visited.add(node)
        pending.extend(dep['pkg'] for dep in nodes[node]['deps']
                       if any(kind['kind'] != 'dev' for kind in dep['dep_kinds']))
    files, index = {}, ['# Third-party notices', '', 'Declared licenses and bundled license texts.', '']
    for identifier in sorted(visited - {root}):
        package = packages[identifier]
        base = pathlib.Path(package['manifest_path']).parent.resolve()
        paths = {path for path in base.rglob('*') if path.is_file()
                 and path.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE'))}
        if package.get('license_file'):
            paths.add(base / package['license_file'])
        if not paths or not package.get('license'):
            raise ValueError(f'missing license metadata/text: {package["name"]}')
        label = f'{package["name"]}-{package["version"]}'
        index.append(f'- {label}: {package["license"]}; source: https://crates.io/crates/{package["name"]}/{package["version"]}')
        for path in sorted(paths):
            if path.is_symlink() or not path.resolve().is_relative_to(base):
                raise ValueError('license file escapes dependency source')
            files[f'THIRD_PARTY_LICENSES/{label}/{path.relative_to(base)}'] = (path.read_bytes(), False)
    return files, index


def check_binary(target, compiler, binary):
    host = next(line.removeprefix('host: ') for line in compiler.splitlines() if line.startswith('host: '))
    if host != target and not (target.endswith('-linux-musl')
                               and host == target.removesuffix('musl') + 'gnu'):
        raise ValueError('build and test each archive on its native OS and architecture')
    if target.endswith('-linux-musl'):
        with binary.open('rb') as source:
            header = source.read(20)
        machine = 62 if target.startswith('x86_64-') else 183
        if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != machine:
            raise ValueError('binary is not ELF64 for the advertised architecture')
        linkage = subprocess.check_output(['readelf', '--program-headers', '--dynamic', str(binary)],
                                          text=True, env={**os.environ, 'LC_ALL': 'C'}, timeout=10)
        if 'INTERP' in linkage or '(NEEDED)' in linkage:
            raise ValueError('Linux archives must be static, without a dynamic loader or shared libraries')


def runtime_licenses(target, compiler):
    if not target.endswith('-linux-musl'):
        return {}, []
    if not compiler.startswith('rustc 1.88.0 '):
        raise ValueError('review bundled musl notices when changing the release compiler')
    directory = ROOT / 'packaging/licenses'
    names = ('musl-1.2.3.COPYRIGHT', 'musl-1.2.3-SOURCE-NOTICES.txt')
    files = {f'THIRD_PARTY_LICENSES/musl/{name}': ((directory / name).read_bytes(), False)
             for name in names}
    return files, ['- musl 1.2.3 (Rust 1.88.0 runtime): see THIRD_PARTY_LICENSES/musl/.']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', choices=TARGETS, required=True)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--output', type=pathlib.Path, default=ROOT / 'dist')
    parser.add_argument('--allow-dirty', action='store_true', help='local preview only; cannot be published')
    args = parser.parse_args()
    dirty = bool(run('git', 'status', '--porcelain'))
    if dirty and not args.allow_dirty:
        parser.error('a clean checkout is required (or --allow-dirty for a local preview)')
    compiler = run('rustc', '-vV')
    package = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']
    version, binary = package['version'], args.binary.resolve()
    check_binary(args.target, compiler, binary)
    if run(str(binary), '--version') != f'tell-tail {version}':
        parser.error('binary version does not match Cargo.toml')
    output = subprocess.check_output([str(binary), '--output', 'json'],
                                     input=b'INFO first\nWARN second\nERROR third\n{bad}\n', timeout=5)
    snapshot = json.loads(output)
    if (snapshot['totals']['records'] != 4 or snapshot['totals']['parse_failures'] != 1
            or snapshot['totals']['levels']['warn'] != 1 or snapshot['totals']['levels']['error'] != 1):
        parser.error('native binary fixture failed')
    metadata = json.loads(run('cargo', 'metadata', '--locked', '--format-version', '1', '--filter-platform', args.target))
    files, index = licenses(metadata)
    runtime_files, runtime_index = runtime_licenses(args.target, compiler)
    files.update(runtime_files)
    index.extend(runtime_index)
    sysroot = pathlib.Path(run('rustc', '--print', 'sysroot'))
    standard_library = sysroot / 'share/doc/rust/COPYRIGHT-library.html'
    files['THIRD_PARTY_LICENSES/rust/COPYRIGHT-library.html'] = (standard_library.read_bytes(), False)
    index.extend(['', f'Rust standard library: {compiler.splitlines()[0]}; see THIRD_PARTY_LICENSES/rust/.', ''])
    files['THIRD_PARTY_NOTICES.md'] = ('\n'.join(index).encode(), False)
    commit = run('git', 'rev-parse', 'HEAD')
    build = {'schema_version': 1, 'version': version, 'source_commit': commit, 'dirty': dirty,
             'target': args.target, 'compiler': compiler, 'build_platform': platform.platform(),
             'linkage': 'static-musl' if args.target.endswith('-linux-musl') else 'system'}
    files['BUILD.json'] = ((json.dumps(build, indent=2) + '\n').encode(), False)
    files['tell-tail'] = (binary.read_bytes(), True)
    for name in ('LICENSE', 'README.md', 'CHANGELOG.md'):
        files[name] = ((ROOT / name).read_bytes(), False)
    name = archive_name(version, args.target)
    prefix = name.removesuffix('.tar.gz')
    args.output.mkdir(parents=True, exist_ok=True)
    path = args.output / name
    write_archive(path, {f'{prefix}/{key}': value for key, value in files.items()},
                  int(run('git', 'show', '-s', '--format=%ct', 'HEAD')))
    record = {**{key: build[key] for key in ('version', 'source_commit', 'dirty')},
              'asset': {'target': args.target, 'name': name, 'sha256': digest(path), 'size': path.stat().st_size}}
    (args.output / f'manifest-{args.target}.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2))


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Resolve a build candidate; publication additionally requires main and validation."""
import argparse
import json
import pathlib
import re
import subprocess
import tomllib

from release_gate import VERSION, validate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--publish', choices=('true', 'false'), default='false')
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.commit):
        parser.error('use a full candidate commit SHA')
    subprocess.run(['git', 'cat-file', '-e', args.commit + '^{commit}'], check=True)
    if args.publish == 'true':
        subprocess.run(['git', 'merge-base', '--is-ancestor', args.commit, 'origin/main'], check=True)
    source = subprocess.check_output(['git', 'show', f'{args.commit}:Cargo.toml'], text=True)
    version = tomllib.loads(source)['package']['version']
    if not re.fullmatch(VERSION, version):
        parser.error('candidate version must be stable x.y.z')
    if args.publish == 'true':
        validate(json.loads(pathlib.Path('packaging/validation.json').read_text()), version, args.commit)
    with args.output.open('a') as output:
        output.write(f'commit={args.commit}\nversion={version}\n')
    print(f'{"Publication approved" if args.publish == "true" else "Build only"}: {version} at {args.commit}')


if __name__ == '__main__':
    main()

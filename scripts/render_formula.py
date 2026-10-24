#!/usr/bin/env python3
"""Generate a Homebrew formula from a verified release manifest."""
import argparse
import json
import pathlib

from release_artifacts import FORMULA_TARGETS, validate_manifest

ROOT = pathlib.Path(__file__).resolve().parents[1]


def render(manifest, template):
    values = {'VERSION': manifest['version']}
    for target, prefix in FORMULA_TARGETS.items():
        asset = next(asset for asset in manifest['assets'] if asset['target'] == target)
        values[f'{prefix}_URL'] = (f'https://github.com/zuchka/tell-tail/releases/download/'
                                    f'v{manifest["version"]}/{asset["name"]}')
        values[f'{prefix}_SHA'] = asset['sha256']
    for name, value in values.items():
        template = template.replace(f'@{name}@', value)
    if '@' in template:
        raise ValueError('unresolved formula placeholder')
    return template


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assets', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    manifest = json.loads((args.assets / 'release.json').read_text())
    validate_manifest(manifest, args.assets)
    if manifest['dirty']:
        parser.error('a dirty candidate cannot become a Homebrew formula')
    template = (ROOT / 'packaging/tell-tail.rb.in').read_text()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(render(manifest, template))


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Publish validated artifacts and resume completed channels without replacing them."""
import argparse
import hashlib
import io
import json
import os
import pathlib
import subprocess
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request

from release_artifacts import verify
from release_gate import validate

TAP = 'ding-labs/homebrew-tap'
API_VERSION = '2026-03-10'


def gh(*args, payload=None):
    command = ['gh', *args]
    return subprocess.check_output(command, input=None if payload is None else json.dumps(payload), text=True)


def api(path, payload=None):
    args = ['api', '-H', f'X-GitHub-Api-Version: {API_VERSION}', path]
    if payload is not None:
        args.extend(['--method', 'POST', '--input', '-'])
    return json.loads(gh(*args, payload=payload))


def optional_api(path):
    """Only a confirmed 404 means absent; auth/server/network failures must stop."""
    result = subprocess.run(['gh', 'api', '-H', f'X-GitHub-Api-Version: {API_VERSION}', path],
                            capture_output=True, text=True)
    if result.returncode == 0:
        return json.loads(result.stdout)
    try:
        status = json.loads(result.stdout).get('status')
    except (ValueError, AttributeError):
        status = None
    if str(status) == '404':
        return None
    raise RuntimeError(f'GitHub API request failed: {path}: {result.stderr.strip()}')


def find_release(repo, tag):
    # The tag endpoint promises published releases only. Authenticated listings
    # also include drafts, which are essential for resuming an interrupted upload.
    pages = json.loads(gh('api', '--paginate', '--slurp', '-H',
                          f'X-GitHub-Api-Version: {API_VERSION}', f'repos/{repo}/releases?per_page=100'))
    matches = [release for page in pages for release in page if release['tag_name'] == tag]
    if len(matches) > 1:
        raise ValueError('multiple releases use this tag; inspect them before retrying')
    return matches[0] if matches else None


def check_identity(manifest, version, commit):
    if manifest['dirty'] or manifest['version'] != version or manifest['source_commit'] != commit:
        raise ValueError('published artifacts do not identify the approved candidate')


def check_release(release, commit, tag):
    if release['tag_name'] != tag or release['prerelease']:
        raise ValueError('existing release is not the requested stable release')
    ref = optional_api(f'repos/{os.environ["GITHUB_REPOSITORY"]}/git/ref/tags/{tag}')
    if ref is not None:
        obj = ref['object']
        if obj['type'] != 'commit' or obj['sha'] != commit:
            raise ValueError('existing tag does not identify the approved candidate')
    elif not release['draft']:
        raise ValueError('published release has no commit tag')


def release_body(record, version, commit):
    validate(record, version, commit)
    waiver = record.get('waivers', {}).get('linux_day')
    if not waiver:
        return ''
    return (f"Validation exception for {version}: the two 24-hour Linux soak scenarios "
            f"were explicitly waived by {waiver['approved_by']}. These scenarios were not passed.\n\n"
            f"Reason: {waiver['reason']}\n\n")


def github_release(directory, version, commit, notes=''):
    manifest = verify(directory)
    check_identity(manifest, version, commit)
    repo = os.environ['GITHUB_REPOSITORY']
    tag = f'v{version}'
    release = find_release(repo, tag)
    if release is None:
        # Never take over a tag created outside this workflow.
        if optional_api(f'repos/{repo}/git/ref/tags/{tag}') is not None:
            raise ValueError('tag already exists without a release; inspect it before publishing')
        release = api(f'repos/{repo}/releases', {'tag_name': tag, 'target_commitish': commit,
                      'draft': True, 'name': f'Tell-Tail {version}', 'generate_release_notes': True, 'body': notes})
    check_release(release, commit, tag)
    if release['draft']:
        # A draft without a tag must still name our full candidate SHA.
        if release['target_commitish'] != commit:
            raise ValueError('draft targets another candidate')
        expected = {asset['name'] for asset in manifest['assets']} | {'release.json', 'SHA256SUMS'}
        assets = {asset['name']: asset for asset in release['assets']}
        if set(assets) - expected:
            raise ValueError('draft contains unexpected assets; inspect it before retrying')
        # Existing draft files must match exactly; upload missing files only.
        for name in sorted(expected):
            if name in assets:
                data = subprocess.check_output(['gh', 'api', '-H', 'Accept: application/octet-stream',
                                                f'repos/{repo}/releases/assets/{assets[name]["id"]}'])
                if hashlib.sha256(data).hexdigest() != hashlib.sha256((directory / name).read_bytes()).hexdigest():
                    raise ValueError(f'draft asset differs: {name}; inspect it before retrying')
            else:
                gh('release', 'upload', tag, str(directory / name), '--repo', repo)
        gh('release', 'edit', tag, '--repo', repo, '--draft=false', '--latest')
    # Published assets are canonical on retries: rebuilding can change binary bytes.
    # Verify their complete checksums and source rather than overwriting any asset.
    with tempfile.TemporaryDirectory(prefix='tell-tail-published-') as temporary:
        remote = pathlib.Path(temporary)
        gh('release', 'download', tag, '--repo', repo, '--dir', str(remote),
           '--pattern', '*.tar.gz', '--pattern', 'release.json', '--pattern', 'SHA256SUMS')
        check_identity(verify(remote), version, commit)
    print(f'GitHub release verified: https://github.com/{repo}/releases/tag/{tag}', flush=True)


def fetch(url):
    request = urllib.request.Request(url, headers={
        'User-Agent': 'tell-tail-release-workflow (https://github.com/ding-labs/tell-tail)'})
    return urllib.request.urlopen(request, timeout=60)


def verify_crate_archive(data, checksum, version, commit):
    if hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError('registry crate checksum mismatch')
    prefix = f'tell-tail-{version}/'
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        vcs = json.load(archive.extractfile(prefix + '.cargo_vcs_info.json'))
        package = tomllib.loads(archive.extractfile(prefix + 'Cargo.toml').read().decode())['package']
    if (vcs['git']['sha1'] != commit or vcs['git'].get('dirty', False)
            or vcs.get('path_in_vcs') != '' or package['name'] != 'tell-tail' or package['version'] != version):
        raise ValueError('registry version belongs to a different source; versions cannot be replaced')


def crate_exists(version, commit):
    try:
        with fetch(f'https://crates.io/api/v1/crates/tell-tail/{version}') as response:
            record = json.load(response)['version']
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return False
        raise
    if record['num'] != version or record['yanked']:
        raise ValueError('registry version is unexpected or yanked')
    with fetch(f'https://crates.io/api/v1/crates/tell-tail/{version}/download') as response:
        verify_crate_archive(response.read(), record['checksum'], version, commit)
    return True


def publish_tap(version, timeout=3600):
    # API 2026-03-10 returns the exact dispatch run ID, avoiding ambiguous run searches.
    result = api(f'repos/{TAP}/actions/workflows/tell-tail.yml/dispatches',
                 {'ref': 'main', 'inputs': {'tag': f'v{version}'}})
    run_id = result['workflow_run_id']
    print(f'Homebrew import: {result["html_url"]}', flush=True)
    deadline = time.monotonic() + timeout
    previous = None
    while time.monotonic() < deadline:
        run = api(f'repos/{TAP}/actions/runs/{run_id}')
        state = (run['status'], run['conclusion'])
        if state != previous:
            print(f'Homebrew run {run_id}: {state}', flush=True)
            previous = state
        if run['status'] == 'completed':
            if run['conclusion'] != 'success':
                raise RuntimeError(f'Homebrew import failed: {result["html_url"]}')
            return
        time.sleep(15)
    raise TimeoutError(f'Homebrew import is still pending; inspect {result["html_url"]} before retrying')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('channel', choices=('github', 'crate-status', 'verify-crate', 'homebrew'))
    parser.add_argument('--version', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--assets', type=pathlib.Path, default=pathlib.Path('dist'))
    parser.add_argument('--validation', type=pathlib.Path, default=pathlib.Path('packaging/validation.json'))
    args = parser.parse_args()
    # Validate inputs even when invoked independently from the workflow.
    from release_gate import VERSION
    import re
    if not re.fullmatch(VERSION, args.version) or not re.fullmatch(r'[0-9a-f]{40}', args.commit):
        parser.error('a stable version and full source SHA are required')
    if args.channel == 'github':
        notes = release_body(json.loads(args.validation.read_text()), args.version, args.commit)
        github_release(args.assets, args.version, args.commit, notes)
    elif args.channel == 'homebrew':
        publish_tap(args.version)
    else:
        exists = crate_exists(args.version, args.commit)
        if args.channel == 'verify-crate' and not exists:
            raise ValueError('crate is not available after publication')
        if args.channel == 'crate-status':
            with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
                output.write(f'exists={str(exists).lower()}\n')
        print(f'Exact-source registry version exists: {exists}', flush=True)


if __name__ == '__main__':
    main()

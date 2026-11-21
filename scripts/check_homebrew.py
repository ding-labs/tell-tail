#!/usr/bin/env python3
"""Install a native candidate through a temporary local tap, without publishing."""
import argparse
import functools
import http.server
import json
import os
import pathlib
import subprocess
import tempfile
import threading

from release_artifacts import TARGETS, archive_name
from render_formula import ROOT, render

TAP = 'tell-tail/release-test'
FORMULA = TAP + '/tell-tail'


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def copyfile(self, source, destination):
        try:
            super().copyfile(source, destination)
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', choices=TARGETS, required=True)
    parser.add_argument('--assets', type=pathlib.Path, required=True)
    args = parser.parse_args()
    record = json.loads((args.assets / f'manifest-{args.target}.json').read_text())
    # Only the native archive is installed. Other-platform URLs remain unused;
    # the final manifest is verified separately during assembly.
    assets = [record['asset']] + [
        {'target': target, 'name': archive_name(record['version'], target), 'sha256': '0' * 64}
        for target in TARGETS if target != args.target]
    manifest = {'version': record['version'], 'assets': assets}
    formula = render(manifest, (ROOT / 'packaging/tell-tail.rb.in').read_text())
    if TAP in subprocess.check_output(['brew', 'tap'], text=True).splitlines():
        parser.error('temporary release-test tap already exists; will not modify it')
    if subprocess.run(['brew', 'list', '--formula', 'tell-tail'], capture_output=True).returncode == 0:
        parser.error('Tell-Tail is already installed; use a clean test runner')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(
        Handler, directory=str(args.assets.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    public_url = (f'https://github.com/zuchka/tell-tail/releases/download/'
                  f'v{record["version"]}/{record["asset"]["name"]}')
    local_url = f'http://127.0.0.1:{server.server_port}/{record["asset"]["name"]}'
    with tempfile.TemporaryDirectory(prefix='tell-tail-brew-config-') as config:
        env = {**os.environ, 'XDG_CONFIG_HOME': config, 'HOMEBREW_NO_AUTO_UPDATE': '1',
               'HOMEBREW_NO_INSTALL_CLEANUP': '1', 'HOMEBREW_NO_ANALYTICS': '1'}
        def brew(*command, **kwargs):
            return subprocess.run(['brew', *command], env=env, check=True, **kwargs)
        created = False
        try:
            brew('tap-new', '--no-git', TAP)
            created = True
            directory = pathlib.Path(subprocess.check_output(['brew', '--repository', TAP], text=True).strip())
            path = directory / 'Formula/tell-tail.rb'
            path.parent.mkdir(exist_ok=True)
            path.write_text(formula)
            brew('style', '--formula', FORMULA)
            path.write_text(formula.replace(public_url, local_url))
            if subprocess.run(['brew', 'command', 'trust'], env=env, capture_output=True).returncode == 0:
                brew('trust', '--formula', FORMULA)
            brew('install', '--build-from-source', FORMULA)
            brew('test', FORMULA)
        finally:
            if created:
                subprocess.run(['brew', 'uninstall', '--formula', FORMULA], env=env, check=False)
                subprocess.run(['brew', 'untap', '--force', TAP], env=env, check=False)
            server.shutdown()
            server.server_close()


if __name__ == '__main__':
    main()

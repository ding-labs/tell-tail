#!/usr/bin/env python3
"""Branch builds must work while publishing unmerged candidates stays blocked."""
import pathlib
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name('release_candidate.py')


class CandidateTests(unittest.TestCase):
    def test_builds_off_main_but_refuses_to_publish_off_main(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)

            def git(*args):
                return subprocess.check_output(['git', *args], cwd=root, text=True, stderr=subprocess.DEVNULL).strip()

            git('init', '--initial-branch=main')
            git('config', 'user.name', 'Release test')
            git('config', 'user.email', 'release-test@example.invalid')
            (root / 'Cargo.toml').write_text('[package]\nname="tell-tail"\nversion="0.2.0"\n')
            git('add', 'Cargo.toml')
            git('commit', '-m', 'base')
            git('update-ref', 'refs/remotes/origin/main', 'HEAD')
            git('switch', '-c', 'candidate')
            (root / 'feature').write_text('unmerged candidate')
            git('add', 'feature')
            git('commit', '-m', 'candidate')
            commit = git('rev-parse', 'HEAD')
            output = root / 'outputs'
            command = [sys.executable, str(SCRIPT), '--commit', commit, '--output', str(output)]
            build = subprocess.run(command, cwd=root, capture_output=True, text=True)
            self.assertEqual(build.returncode, 0, build.stderr)
            self.assertEqual(output.read_text(), f'commit={commit}\nversion=0.2.0\n')
            output.unlink()
            publish = subprocess.run([*command, '--publish', 'true'], cwd=root, capture_output=True)
            self.assertNotEqual(publish.returncode, 0)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()

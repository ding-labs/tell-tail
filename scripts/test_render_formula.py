#!/usr/bin/env python3
import pathlib
import unittest

from release_artifacts import TARGETS, archive_name
from render_formula import render


class FormulaTests(unittest.TestCase):
    def test_formula_pins_each_platform_to_its_release_archive(self):
        manifest = {'version': '0.2.0', 'assets': [
            {'target': target, 'name': archive_name('0.2.0', target), 'sha256': char * 64}
            for target, char in zip(TARGETS, ('a', 'b', 'c'))]}
        template = pathlib.Path(__file__).resolve().parents[1] / 'packaging/tell-tail.rb.in'
        formula = render(manifest, template.read_text())
        self.assertNotIn('@', formula)
        for asset in manifest['assets']:
            self.assertIn(f'/v0.2.0/{asset["name"]}', formula)
            self.assertIn(asset['sha256'], formula)
        self.assertIn('depends_on arch: :arm64', formula)
        self.assertIn('on_intel do', formula)
        self.assertIn('on_arm do', formula)
        self.assertNotIn('unknown-linux-gnu', formula)
        self.assertNotIn('depends_on "rust"', formula)


if __name__ == '__main__':
    unittest.main()

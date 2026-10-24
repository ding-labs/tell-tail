#!/usr/bin/env python3
"""Reject mislabeled or dynamically linked Linux downloads before packaging."""
import pathlib
import tempfile
import unittest
from unittest.mock import patch

from package_release import check_binary, runtime_licenses


class BinaryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.binary = pathlib.Path(self.temp.name) / 'tell-tail'
        self.binary.write_bytes(b'\x7fELF\x02\x01' + b'\0' * 12 + (62).to_bytes(2, 'little'))

    def test_accepts_static_musl_built_on_native_gnu_host(self):
        with patch('package_release.subprocess.check_output', return_value='There is no dynamic section.'):
            check_binary('x86_64-unknown-linux-musl', 'host: x86_64-unknown-linux-gnu', self.binary)

    def test_rejects_shared_libraries_and_dynamic_loader(self):
        for output in ['INTERP 0x00001', '0x00001 (NEEDED) Shared library: [libc.so.6]']:
            with self.subTest(output=output), patch('package_release.subprocess.check_output', return_value=output):
                with self.assertRaisesRegex(ValueError, 'must be static'):
                    check_binary('x86_64-unknown-linux-musl', 'host: x86_64-unknown-linux-gnu', self.binary)

    def test_rejects_wrong_binary_architecture_even_on_native_host(self):
        with self.assertRaisesRegex(ValueError, 'advertised architecture'):
            check_binary('aarch64-unknown-linux-musl', 'host: aarch64-unknown-linux-gnu', self.binary)

    def test_rejects_cross_compiled_candidates(self):
        with self.assertRaisesRegex(ValueError, 'native OS and architecture'):
            check_binary('x86_64-unknown-linux-musl', 'host: aarch64-apple-darwin', self.binary)

    def test_static_runtime_carries_upstream_license_texts(self):
        for target in ('x86_64-unknown-linux-musl', 'aarch64-unknown-linux-musl'):
            files, notices = runtime_licenses(target, 'rustc 1.88.0 (release)')
            self.assertIn('Rich Felker', files['THIRD_PARTY_LICENSES/musl/musl-1.2.3.COPYRIGHT'][0].decode())
            self.assertIn('Arm Limited', files['THIRD_PARTY_LICENSES/musl/musl-1.2.3-SOURCE-NOTICES.txt'][0].decode())
            self.assertTrue(notices)
        with self.assertRaisesRegex(ValueError, 'review bundled musl'):
            runtime_licenses('x86_64-unknown-linux-musl', 'rustc 1.89.0 (release)')
        self.assertEqual(({}, []), runtime_licenses('aarch64-apple-darwin', 'rustc 1.88.0 (release)'))


if __name__ == '__main__':
    unittest.main()

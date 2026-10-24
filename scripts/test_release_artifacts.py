#!/usr/bin/env python3
import json
import pathlib
import tarfile
import tempfile
import unittest

from release_artifacts import TARGETS, archive_name, assemble, digest, validate_manifest, write_archive


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = pathlib.Path(self.temp.name)
        for target in TARGETS:
            name = archive_name('0.2.0', target)
            path = self.directory / name
            path.write_bytes(target.encode())
            record = {'version': '0.2.0', 'source_commit': 'a' * 40, 'dirty': False,
                      'asset': {'target': target, 'name': name, 'sha256': digest(path), 'size': path.stat().st_size}}
            (self.directory / f'manifest-{target}.json').write_text(json.dumps(record))

    def test_manifest_and_checksums_cover_all_targets(self):
        manifest = assemble(self.directory)
        validate_manifest(manifest, self.directory)
        self.assertEqual(len(TARGETS) + 1, len((self.directory / 'SHA256SUMS').read_text().splitlines()))
        (self.directory / manifest['assets'][0]['name']).write_bytes(b'tampered')
        with self.assertRaises(ValueError):
            validate_manifest(manifest, self.directory)

    def test_refuses_mixed_candidates_and_missing_targets(self):
        path = self.directory / f'manifest-{TARGETS[0]}.json'
        data = json.loads(path.read_text())
        data['source_commit'] = 'b' * 40
        path.write_text(json.dumps(data))
        with self.assertRaises(ValueError):
            assemble(self.directory)
        path.unlink()
        with self.assertRaises(ValueError):
            assemble(self.directory)

    def test_refuses_duplicate_architectures(self):
        path = self.directory / f'manifest-{TARGETS[-1]}.json'
        other = self.directory / f'manifest-{TARGETS[0]}.json'
        path.write_bytes(other.read_bytes())
        with self.assertRaises(ValueError):
            assemble(self.directory)

    def test_archives_have_stable_metadata_and_executable_mode(self):
        paths = [self.directory / name for name in ('first.tar.gz', 'second.tar.gz')]
        for path in paths:
            write_archive(path, {'bundle/tell-tail': (b'binary', True), 'bundle/LICENSE': (b'license', False)}, 123)
        self.assertEqual(digest(paths[0]), digest(paths[1]))
        with tarfile.open(paths[0]) as archive:
            self.assertEqual(0o755, archive.getmember('bundle/tell-tail').mode)
            self.assertEqual(0o644, archive.getmember('bundle/LICENSE').mode)
            self.assertTrue(all(member.isfile() for member in archive))


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
import json
import pathlib
import tempfile
import unittest
from watch_docker_soak import reconcile_exit


class WatcherTests(unittest.TestCase):
    def test_abrupt_exit_preserves_partial_journal_and_marks_interruption(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            original = {'source_commit': 'candidate', 'status': 'running',
                        'phases': [{'name': 'stdin-day', 'status': 'running'}]}
            (root / 'campaign.json').write_text(json.dumps(original))
            self.assertEqual(reconcile_exit(root, {'ExitCode': 255}), 1)
            updated = json.loads((root / 'campaign.json').read_text())
            self.assertEqual(updated['source_commit'], 'candidate')
            self.assertEqual(updated['status'], 'interrupted')
            self.assertEqual(updated['phases'][0]['status'], 'interrupted')
            self.assertEqual(json.loads((root / 'campaign.before-interruption.json').read_text()), original)

    def test_zero_exit_does_not_substitute_for_a_completed_journal(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / 'campaign.json').write_text('{"status":"running"}')
            self.assertEqual(reconcile_exit(root, {'ExitCode': 0}), 1)
            self.assertEqual(json.loads((root / 'campaign.json').read_text())['status'], 'interrupted')

    def test_completed_campaign_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            document = '{"status":"passed","phases":[{"status":"passed"}]}'
            (root / 'campaign.json').write_text(document)
            self.assertEqual(reconcile_exit(root, {'ExitCode': 0}), 0)
            self.assertEqual((root / 'campaign.json').read_text(), document)

    def test_application_failure_is_not_overwritten_by_host_diagnosis(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            document = '{"status":"failed","error":"exact-count mismatch"}'
            (root / 'campaign.json').write_text(document)
            self.assertEqual(reconcile_exit(root, {'ExitCode': 1}), 1)
            self.assertEqual((root / 'campaign.json').read_text(), document)


if __name__ == '__main__':
    unittest.main()

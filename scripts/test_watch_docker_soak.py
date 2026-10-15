#!/usr/bin/env python3
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from watch_docker_soak import main, reconcile_exit


class WatcherTests(unittest.TestCase):
    def test_docker_disconnect_is_retried_before_reconciling_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / 'results').mkdir()
            (root / 'config.json').write_text('{"container":"owned-test-container"}')
            (root / 'results/campaign.json').write_text('{"status":"running"}')
            replies = [subprocess.CalledProcessError(1, 'docker', stderr='unexpected EOF'),
                       '{"Running":false,"ExitCode":255}']
            with patch('sys.argv', ['watcher', '--campaign', str(root)]), \
                    patch('watch_docker_soak.subprocess.check_output', side_effect=replies) as docker, \
                    patch('watch_docker_soak.time.sleep') as sleep:
                self.assertEqual(main(), 1)
            self.assertEqual(docker.call_count, 2)
            sleep.assert_called_once_with(15)
            monitor = json.loads((root / 'results/host-monitor.json').read_text())
            self.assertEqual(monitor['status'], 'interrupted')
            before = json.loads((root / 'results/campaign.before-interruption.json').read_text())
            self.assertEqual(before['status'], 'running')

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

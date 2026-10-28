#!/usr/bin/env python3
"""Exercise partial publication, immutable identities, and downstream failures offline."""
import hashlib
import io
import json
import os
import pathlib
import subprocess
import tarfile
import tempfile
import unittest
import urllib.error
from unittest.mock import patch

from release_gate import GATES

import release_publish as publish

SHA = 'a' * 40
VERSION = '0.2.0'
REPO = 'ding-labs/tell-tail'


def crate(commit=SHA, dirty=False):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w:gz') as archive:
        for name, data in {
            '.cargo_vcs_info.json': json.dumps({'git': {'sha1': commit, 'dirty': dirty}, 'path_in_vcs': ''}).encode(),
            'Cargo.toml': b'[package]\nname="tell-tail"\nversion="0.2.0"\n',
        }.items():
            entry = tarfile.TarInfo('tell-tail-0.2.0/' + name)
            entry.size = len(data)
            archive.addfile(entry, io.BytesIO(data))
    return output.getvalue()


class CrateTests(unittest.TestCase):
    def test_existing_crate_requires_matching_source_and_checksum(self):
        data = crate()
        publish.verify_crate_archive(data, hashlib.sha256(data).hexdigest(), VERSION, SHA)
        for data, checksum in [(data, 'b' * 64), (crate('b' * 40), None), (crate(dirty=True), None)]:
            with self.subTest(checksum=checksum), self.assertRaises(ValueError):
                publish.verify_crate_archive(data, checksum or hashlib.sha256(data).hexdigest(), VERSION, SHA)

    def test_only_registry_404_is_absent(self):
        for status in [404, 403, 429, 500]:
            with self.subTest(status=status), patch.object(publish, 'fetch', side_effect=urllib.error.HTTPError(
                    'https://crates.io', status, 'failure', {}, None)):
                if status == 404:
                    self.assertFalse(publish.crate_exists(VERSION, SHA))
                else:
                    with self.assertRaises(urllib.error.HTTPError):
                        publish.crate_exists(VERSION, SHA)

    def test_yanked_or_foreign_crate_cannot_be_skipped(self):
        data = crate('b' * 40)
        for yanked in (False, True):
            record = {'version': {'num': VERSION, 'yanked': yanked, 'checksum': hashlib.sha256(data).hexdigest()}}
            with self.subTest(yanked=yanked), patch.object(publish, 'fetch', side_effect=[
                    io.BytesIO(json.dumps(record).encode()), io.BytesIO(data)]), self.assertRaises(ValueError):
                publish.crate_exists(VERSION, SHA)


class GithubTests(unittest.TestCase):
    def test_release_notes_disclose_the_waiver_and_still_require_other_gates(self):
        record = {'schema_version': 1, 'version': VERSION, 'source_commit': SHA,
                  'gates': {name: {'passed': True, 'evidence': ['result']} for name in GATES}}
        self.assertEqual(publish.release_body(record, VERSION, SHA), '')
        record['gates']['linux_day']['passed'] = False
        record['waivers'] = {'linux_day': {'version': VERSION, 'source_commit': SHA,
            'approved_by': 'maintainer', 'approved_at': '2026-10-28T23:50:00+00:00',
            'reason': 'Explicitly skip the two full-day scenarios.'}}
        notes = publish.release_body(record, VERSION, SHA)
        self.assertIn('explicitly waived by maintainer', notes)
        self.assertIn('were not passed', notes)
        record['gates']['fuzzing']['passed'] = False
        with self.assertRaises(ValueError):
            publish.release_body(record, VERSION, SHA)

    def test_discovers_drafts_across_pages_and_rejects_duplicate_tags(self):
        draft = {'tag_name': 'v0.2.0', 'draft': True}
        for pages, expected in [([[], [draft]], draft), ([[{'tag_name': 'v0.1.0'}]], None)]:
            with patch.object(publish, 'gh', return_value=json.dumps(pages)):
                self.assertEqual(publish.find_release(REPO, 'v0.2.0'), expected)
        with patch.object(publish, 'gh', return_value=json.dumps([[draft], [draft]])), \
                self.assertRaises(ValueError):
            publish.find_release(REPO, 'v0.2.0')

    def test_only_confirmed_404_is_absent(self):
        for status in ['404', '403', '500', None]:
            with self.subTest(status=status), patch.object(publish.subprocess, 'run', return_value=
                    subprocess.CompletedProcess([], 1, json.dumps({'status': status}), 'failed')):
                if status == '404':
                    self.assertIsNone(publish.optional_api('repos/example'))
                else:
                    with self.assertRaises(RuntimeError):
                        publish.optional_api('repos/example')

    @patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
    def test_rejects_moved_or_annotated_tags(self):
        release = {'tag_name': 'v0.2.0', 'prerelease': False, 'draft': False}
        for obj in [{'type': 'commit', 'sha': 'b' * 40}, {'type': 'tag', 'sha': SHA}]:
            with self.subTest(obj=obj), patch.object(publish, 'optional_api', return_value={'object': obj}), \
                    self.assertRaises(ValueError):
                publish.check_release(release, SHA, 'v0.2.0')

    @patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
    def test_public_release_is_verified_without_upload_or_edit(self):
        manifest = {'dirty': False, 'version': VERSION, 'source_commit': SHA}
        release = {'tag_name': 'v0.2.0', 'prerelease': False, 'draft': False}
        with patch.object(publish, 'verify', return_value=manifest), patch.object(publish, 'find_release', return_value=release), \
                patch.object(publish, 'optional_api', return_value={'object': {'type': 'commit', 'sha': SHA}}), \
                patch.object(publish, 'gh') as gh:
            publish.github_release(pathlib.Path('dist'), VERSION, SHA)
        self.assertEqual(len(gh.call_args_list), 1)
        self.assertEqual(gh.call_args.args[:2], ('release', 'download'))

    @patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
    def test_resumes_partial_draft_by_uploading_only_missing_assets(self):
        manifest = {'dirty': False, 'version': VERSION, 'source_commit': SHA,
                    'assets': [{'name': 'native.tar.gz'}]}
        release = {'tag_name': 'v0.2.0', 'prerelease': False, 'draft': True,
                   'target_commitish': SHA, 'assets': [{'name': 'native.tar.gz', 'id': 42}]}
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / 'native.tar.gz').write_bytes(b'archive')
            with patch.object(publish, 'verify', return_value=manifest), patch.object(publish, 'find_release', return_value=release), \
                    patch.object(publish, 'optional_api', return_value=None), patch.object(publish, 'gh') as gh, \
                    patch.object(publish.subprocess, 'check_output', return_value=b'archive'):
                publish.github_release(root, VERSION, SHA)
            uploads = [call.args[3] for call in gh.call_args_list if call.args[:2] == ('release', 'upload')]
            self.assertEqual({pathlib.Path(path).name for path in uploads}, {'release.json', 'SHA256SUMS'})
            self.assertTrue(any(call.args[:2] == ('release', 'edit') for call in gh.call_args_list))

    @patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
    def test_draft_mismatch_fails_before_editing(self):
        manifest = {'dirty': False, 'version': VERSION, 'source_commit': SHA,
                    'assets': [{'name': 'native.tar.gz'}]}
        release = {'tag_name': 'v0.2.0', 'prerelease': False, 'draft': True,
                   'target_commitish': 'b' * 40, 'assets': []}
        with patch.object(publish, 'verify', return_value=manifest), patch.object(publish, 'find_release', return_value=release), \
                patch.object(publish, 'optional_api', return_value=None), patch.object(publish, 'gh') as gh, self.assertRaises(ValueError):
            publish.github_release(pathlib.Path('dist'), VERSION, SHA)
        gh.assert_not_called()

    @patch.dict(os.environ, GITHUB_REPOSITORY=REPO)
    def test_existing_tag_without_release_is_never_taken_over(self):
        manifest = {'dirty': False, 'version': VERSION, 'source_commit': SHA}
        with patch.object(publish, 'verify', return_value=manifest), patch.object(publish, 'find_release', return_value=None), \
                patch.object(publish, 'optional_api', return_value={'object': {'sha': SHA}}), patch.object(publish, 'gh') as gh, \
                self.assertRaises(ValueError):
            publish.github_release(pathlib.Path('dist'), VERSION, SHA)
        gh.assert_not_called()


class TapTests(unittest.TestCase):
    def test_waits_for_the_dispatched_run_and_propagates_failure(self):
        for conclusion in ('success', 'failure', 'cancelled'):
            with self.subTest(conclusion=conclusion), patch.object(publish, 'api', side_effect=[
                    {'workflow_run_id': 123, 'html_url': 'https://github.com/run/123'},
                    {'status': 'queued', 'conclusion': None},
                    {'status': 'completed', 'conclusion': conclusion}]) as api, \
                    patch.object(publish.time, 'sleep'):
                if conclusion == 'success':
                    publish.publish_tap(VERSION)
                else:
                    with self.assertRaises(RuntimeError):
                        publish.publish_tap(VERSION)
                self.assertEqual(api.call_args.args[0], f'repos/{publish.TAP}/actions/runs/123')
                self.assertEqual(api.call_args_list[0].args[1], {'ref': 'main', 'inputs': {'tag': 'v0.2.0'}})

    def test_pending_run_times_out_with_recovery_link(self):
        with patch.object(publish, 'api', return_value={
                'workflow_run_id': 123, 'html_url': 'https://github.com/run/123'}), \
                self.assertRaisesRegex(TimeoutError, 'https://github.com/run/123'):
            publish.publish_tap(VERSION, timeout=0)


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env python3
"""Regression checks for publication gates; no release is created."""
import copy
import json
import pathlib
import unittest

from release_gate import GATES, validate

SHA = 'a' * 40


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        self.record = {'schema_version': 1, 'version': '0.2.0', 'source_commit': SHA,
                       'gates': {name: {'passed': True, 'evidence': ['docs/validation/result.json']}
                                 for name in GATES}}

    def test_accepts_complete_matching_evidence(self):
        validate(self.record, '0.2.0', SHA)

    def test_current_repository_cannot_publish(self):
        path = pathlib.Path(__file__).resolve().parents[1] / 'packaging/validation.json'
        with self.assertRaises(ValueError):
            validate(json.loads(path.read_text()), '0.2.0', SHA)

    def test_rejects_stale_identity_and_prereleases(self):
        for version, commit in [('0.2.1', SHA), ('0.2.0', 'b' * 40), ('0.2.0-rc.1', SHA),
                                ('0.2.0', 'a' * 7), ('00.2.0', SHA)]:
            with self.subTest(version=version, commit=commit), self.assertRaises(ValueError):
                validate(self.record, version, commit)

    def test_rejects_missing_gate_or_unsubstantiated_pass(self):
        for missing in GATES:
            record = copy.deepcopy(self.record)
            del record['gates'][missing]
            with self.assertRaises(ValueError):
                validate(record, '0.2.0', SHA)
        for passed, evidence in [(False, ['result']), ('true', ['result']), (True, []),
                                 (True, ['']), (True, 'result')]:
            record = copy.deepcopy(self.record)
            record['gates']['performance'] = {'passed': passed, 'evidence': evidence}
            with self.assertRaises(ValueError):
                validate(record, '0.2.0', SHA)


if __name__ == '__main__':
    unittest.main()

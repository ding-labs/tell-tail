"""The day gate must reject partial durations, changed binaries and interruptions."""
import copy
import unittest
from runner import review


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.identity = {'source_commit': 'a' * 40, 'binary_sha256': 'b' * 64}
        self.report = {'status': 'passed', 'commit': 'a' * 40, 'harness_commit': 'a' * 40,
                       'workspace_dirty': False, 'harness_dirty': False,
                       'binary_sha256': 'b' * 64, 'configuration': {'seconds': 86400},
                       'production_seconds': 86400.01, 'max_observation_gap_seconds': .1,
                       'peak_sampled_rss_mib': 8, 'producer_saturation_warning': False,
                       'resource_samples': 86000, 'snapshot_age_p95_upper_ms': 250,
                       'rss_10minute_medians_mib': [8] * 143}

    def test_complete_day(self):
        review(self.report, self.identity, 86400)

    def test_invalid_evidence(self):
        for key, value in [('production_seconds', 86399), ('commit', 'c' * 40),
                           ('binary_sha256', 'c' * 64), ('harness_dirty', True),
                           ('max_observation_gap_seconds', 5.01),
                           ('producer_saturation_warning', True),
                           ('rss_10minute_medians_mib', [8, 8, 13] * 48),
                           ('rss_10minute_medians_mib', [8] * 100)]:
            with self.subTest(key=key, value=value):
                report = copy.deepcopy(self.report)
                report[key] = value
                with self.assertRaises(AssertionError):
                    review(report, self.identity, 86400)


if __name__ == '__main__':
    unittest.main()

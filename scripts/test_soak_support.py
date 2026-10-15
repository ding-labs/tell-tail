#!/usr/bin/env python3
import copy
import unittest
from soak_support import Counts, ObservationClock, cpu_seconds, make_batch, select_tree


class SupportTests(unittest.TestCase):
    def test_continuity_rejects_suspend_even_when_monotonic_clock_pauses(self):
        clock = ObservationClock(0, 100)
        clock.observe(1, 101)
        clock.observe(2, 102)
        self.assertEqual(clock.maximum_gap, 1)
        with self.assertRaisesRegex(AssertionError, 'observation gap'):
            clock.observe(2.1, 110)
        self.assertEqual(clock.maximum_gap, 8)
        clock = ObservationClock(0, 100)
        with self.assertRaisesRegex(AssertionError, 'observation gap'):
            clock.observe(6, 106)

    def test_metrics_reader_rejects_partial_or_duplicate_reports(self):
        from soak import metrics_report
        lines = [f'tell_tail_records_total{{level="{level}"}} 0' for level in
                 ('trace', 'debug', 'info', 'warn', 'error', 'fatal', 'unknown')]
        lines += [f'tell_tail_{name} 0' for name in ('parse_failures_total',
                  'oversized_records_total', 'bytes_total', 'pending_bytes',
                  'skipped_bytes_total', 'discarded_partial_records_total',
                  'source_up', 'running', 'snapshot_timestamp_seconds')]
        document = '\n'.join(lines) + '\n'
        Counts().verify(metrics_report(document))
        for bad in [document[:-1], document + lines[0] + '\n', '\n'.join(lines[1:]) + '\n']:
            with self.assertRaises(AssertionError):
                metrics_report(bad)

    def test_sustained_fixture_has_exact_distribution_and_size(self):
        payload, counts, digest = make_batch(7, 'sustained', 1024)
        self.assertEqual(len(payload), 20000)
        self.assertEqual(counts.levels['info'], 90)
        self.assertEqual(counts.levels['warn'], 9)
        self.assertEqual(counts.levels['error'], 1)
        self.assertEqual(make_batch(7, 'sustained', 1024)[2], digest)
        self.assertNotEqual(make_batch(8, 'sustained', 1024)[2], digest)

    def test_mixed_fixture_and_accounting_detect_mismatches(self):
        payload, counts, _ = make_batch(1, 'mixed', 1024)
        self.assertEqual(counts.records, 100)
        self.assertEqual(counts.bytes_read, len(payload))
        self.assertEqual(counts.parse_failures, 20)
        self.assertEqual(counts.oversized_records, 1)
        for level in ('trace', 'debug', 'info', 'warn', 'error', 'fatal'):
            self.assertEqual(counts.levels[level], 10)
        report = {'totals': {'levels': counts.levels.copy(), 'records': 100,
                            'parse_failures': 20, 'oversized_records': 1},
                  'bytes_read': len(payload), 'skipped_bytes': 0,
                  'discarded_partial_records': 0, 'pending_bytes': 0}
        counts.verify(report)
        wrong = copy.deepcopy(report)
        wrong['totals']['levels']['error'] += 1
        with self.assertRaises(AssertionError):
            counts.verify(wrong)
        total = Counts()
        total.add(counts)
        total.add(counts)
        self.assertEqual(total.records, 200)
        self.assertEqual(total.bytes_read, len(payload) * 2)

    def test_tree_includes_helpers_and_excludes_other_processes(self):
        rows = [(1, 0, 1, 0), (2, 1, 2, .1), (3, 2, 3, .2), (4, 1, 4, .3)]
        self.assertEqual(select_tree(rows, 2), rows[1:3])
        self.assertEqual(cpu_seconds('1-02:03:04.50'), 93784.5)
        self.assertEqual(cpu_seconds('00:02.25'), 2.25)
        self.assertEqual(cpu_seconds('00:01:02'), 62)


if __name__ == '__main__':
    unittest.main()

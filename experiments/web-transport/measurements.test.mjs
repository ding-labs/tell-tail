import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyze, distribution, parseProcesses, processTree, resources } from './measurements.mjs';

test('nearest-rank quantiles retain negative timing values and reject invalid samples', () => {
  assert.deepEqual(distribution([3, -1, 1, 2]), { n: 4, min: -1, p50: 1, p95: 3, p99: 3, max: 3 });
  assert.equal(distribution([]), null);
  assert.throws(() => distribution([NaN]));
  const measured = analyze({ samples: [{ published_us: 1000, received_ms: 10.9, decode_ms: .1, dom_ms: 12, dom_work_ms: .2 }],
    calibration: { offset_ms: 10, rtt_ms: .4 }, visibility: 'visible', received: 1, rendered: 1, missed: 0 }, { offset_ms: 10.1 });
  assert.ok(measured.receipt_ms.min < 0, 'do not hide calibration error by clamping');
  assert.equal(measured.dom_ms.p95, 1);
  assert.equal(measured.clock_uncertainty_ms, .2);
  assert.throws(() => analyze({ samples: [], visibility: 'visible' }, {}));
});

test('process-tree resources include descendants without charging baseline CPU', () => {
  const before = parseProcesses('1 0 100 00:01.00\n2 1 200 01:01.00\n3 9 300 00:05.00');
  const after = parseProcesses('1 0 110 00:02.00\n2 1 210 01:03.00\n3 9 300 00:05.00\n4 2 50 00:00.50');
  assert.deepEqual(resources(processTree(before, 1), processTree(after, 1), [processTree(before, 1), processTree(after, 1)]),
    { cpu_seconds: 3.5, rss_peak_kib: 370, exited_processes: 0 });
});

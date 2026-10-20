import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { elapsed, historyPoints, integer, parseFrame, percent, rate } from '../web/frame.mjs';

const fixture = readFileSync(new URL('../tests/fixtures/output/empty-web-v1.json', import.meta.url), 'utf8');

test('production fixture preserves nulls, zeroes and exact counters', () => {
  const frame = parseFrame(fixture);
  assert.equal(frame.sequence, '0');
  assert.equal(frame.metrics.window.warning_ratio, null);
  assert.equal(integer('18446744073709551615'), '18,446,744,073,709,551,615');
  assert.equal(integer('0'), '0');
  assert.equal(percent(null), '—');
  assert.equal(percent(0), '0%');
  assert.equal(percent(0.125), '12.5%');
  assert.equal(rate(1234.55), '1,234.6');
  assert.equal(elapsed(3661.9), '01:01:01');
});

test('completed-second history replaces snapshots and retains exact labels', () => {
  const m = parseFrame(fixture).metrics;
  m.elapsed_seconds = 64.9;
  m.history = ['0', '18446744073709551615'];
  assert.deepEqual(historyPoints(m), [
    { second: 62, count: '0', value: 0 },
    { second: 63, count: '18446744073709551615', value: Number('18446744073709551615') },
  ]);
  m.history = ['2'];
  assert.deepEqual(historyPoints(m), [{ second: 63, count: '2', value: 2 }]);
});

test('invalid versions, states, numbers, counter encodings and sizes fail clearly', () => {
  const mutations = [
    f => f.web_schema_version = 2,
    f => f.sequence = 0,
    f => f.sequence = '01',
    f => f.sequence = '18446744073709551616',
    f => f.metrics.state = 'connected',
    f => f.metrics.kind = 'snapshot',
    f => f.metrics.source = null,
    f => f.metrics.session_id = 'bad\nID',
    f => f.metrics.totals.levels.info = -1,
    f => f.metrics.window.error_ratio = 1.1,
    f => f.metrics.window.observed_seconds = 61,
    f => f.metrics.window.records_per_second = null,
    f => f.metrics.window.configured_seconds = 0,
    f => f.metrics.history = Array(61).fill('1'),
    f => f.metrics.source = '🦀'.repeat(17000),
  ];
  for (const mutate of mutations) {
    const frame = JSON.parse(fixture);
    mutate(frame);
    assert.throws(() => parseFrame(JSON.stringify(frame)), /Unsupported or invalid/);
  }
  for (const malformed of ['null', '{}', '{', '[]']) {
    assert.throws(() => parseFrame(malformed), /Unsupported or invalid/);
  }
});

// Stats retains the whole oldest 100 ms bucket, including its partial edge.
test('quantized rolling windows remain valid after filling and wrapping', () => {
  for (const seconds of [1, 60, 3600]) {
    for (const observed of [seconds - 0.001, seconds, seconds + 0.001, seconds + 0.099999999]) {
      const frame = JSON.parse(fixture);
      frame.metrics.window.configured_seconds = seconds;
      frame.metrics.window.observed_seconds = observed;
      assert.equal(parseFrame(JSON.stringify(frame)).metrics.window.observed_seconds, observed);
    }
    for (const observed of [seconds + 0.1, seconds + 1]) {
      const frame = JSON.parse(fixture);
      frame.metrics.window.configured_seconds = seconds;
      frame.metrics.window.observed_seconds = observed;
      assert.throws(() => parseFrame(JSON.stringify(frame)), /Unsupported or invalid/);
    }
  }
});

test('validation errors identify fields without echoing untrusted payloads', () => {
  for (const [mutate, field] of [
    [f => f.metrics.window.observed_seconds = 61, 'metrics.window.observed_seconds'],
    [f => f.metrics.totals.levels.info = 'private log content', 'metrics.totals.levels.info'],
    [f => f.metrics.window = null, 'metrics.window'],
    [f => f.metrics.history = ['secret'], 'metrics.history[]'],
  ]) {
    const f = JSON.parse(fixture);
    mutate(f);
    assert.throws(() => parseFrame(JSON.stringify(f)), error => {
      assert.equal(error.name, 'FrameError');
      assert.equal(error.field, field);
      assert.doesNotMatch(error.message, /private|secret/);
      return true;
    });
  }
  assert.throws(() => parseFrame('{"secret'), /frame must be valid JSON/);
});

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { activityText, elapsed, historyPoints, integer, parseFrame, parsingCoverage, percent, rate, severityComparison } from '../web/frame.mjs';

const fixture = readFileSync(new URL('../tests/fixtures/output/empty-web-v1.json', import.meta.url), 'utf8');

test('activity distinguishes absent observations, idle ages and waiting states', () => {
  const frame = JSON.parse(fixture);
  assert.equal(activityText(frame).record, 'None observed');
  frame.metrics.state = 'following'; frame.metrics.kind = 'snapshot';
  assert.match(activityText(frame).note, /--from start/);
  frame.metrics.pending_bytes = '8';
  assert.match(activityText(frame).note, /partial record/);
  frame.metrics.state = 'waiting';
  assert.match(activityText(frame).note, /file to appear/);
  frame.metrics.elapsed_seconds = 12;
  frame.metrics.totals.records = '1';
  frame.activity = {last_record_age_seconds: 10, last_error_age_seconds: null};
  assert.equal(activityText(frame).record, '10s ago');
  parseFrame(JSON.stringify(frame));
  for (const age of [-1, 13, '1']) {
    frame.activity.last_record_age_seconds = age;
    assert.throws(() => parseFrame(JSON.stringify(frame)), /activity/);
  }
  delete frame.activity;
  parseFrame(JSON.stringify(frame));
  assert.equal(activityText(frame).record, 'Unavailable');
});

test('severity history validates alignment and compares two complete intervals', () => {
  const frame = JSON.parse(fixture);
  assert.match(severityComparison(frame), /needs 60/);
  frame.metrics.history = Array(60).fill('10');
  frame.severity_history = Array.from({ length: 60 }, (_, i) => ({ warn: '1', error: i < 30 ? '0' : '2' }));
  parseFrame(JSON.stringify(frame));
  assert.match(severityComparison(frame), /last 30 completed seconds: 60 \/ 300 \(20%\); preceding 30: 0 \/ 300 \(0%\)/);
  for (const mutate of [
    f => f.severity_history.pop(),
    f => f.severity_history[0].warn = 1,
    f => f.severity_history[0].error = '11',
  ]) {
    const bad = structuredClone(frame);
    mutate(bad);
    assert.throws(() => parseFrame(JSON.stringify(bad)), /severity_history/);
  }
  frame.metrics.history.fill('0');
  frame.severity_history = Array.from({ length: 60 }, () => ({ warn: '0', error: '0' }));
  assert.match(severityComparison(frame), /0 \/ 0 \(no records\)/);
});

test('coverage distinguishes no observations from unrecognized and malformed records', () => {
  const counts = JSON.parse(fixture).metrics.window.counts;
  assert.match(parsingCoverage(counts).summary, /unavailable/);
  Object.assign(counts, { records: '4', parse_failures: '1', oversized_records: '1' });
  counts.levels.unknown = '3';
  assert.equal(parsingCoverage(counts).summary, 'Severity recognized: 25% · 1 of 4 rolling records');
  assert.equal(parsingCoverage(counts).breakdown, '3 unknown: 1 malformed, 1 oversized, 1 without a recognized severity.');
  counts.records = '18446744073709551615';
  assert.match(parsingCoverage(counts).summary, /18,446,744,073,709,551,612 of 18,446,744,073,709,551,615/);
});

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

test('optional error previews validate bounds and exact record numbers', () => {
  const frame = JSON.parse(fixture);
  const entry = { record: '18446744073709551615', elapsed_seconds: 1.5, level: 'fatal', preview: '<script>text</script>', truncated: false };
  frame.recent_errors = [entry];
  assert.deepEqual(parseFrame(JSON.stringify(frame)).recent_errors, [entry]);
  for (const change of [
    f => f.recent_errors = null,
    f => f.recent_errors = Array(21).fill(entry),
    f => f.recent_errors[0].preview = 'é'.repeat(257),
    f => f.recent_errors[0].record = 1,
    f => f.recent_errors[0].level = 'warn',
    f => f.recent_errors[0].elapsed_seconds = -1,
    f => f.recent_errors[0].truncated = 'false',
  ]) {
    const bad = structuredClone(frame);
    change(bad);
    assert.throws(() => parseFrame(JSON.stringify(bad)), /Unsupported or invalid/);
  }
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

test('parser evidence validates exact counts and provides specific corrections', async () => {
  const { parsingEvidence } = await import('../web/frame.mjs');
  const frame = JSON.parse(fixture);
  frame.metrics.totals.records = '4'; frame.metrics.totals.levels.unknown = '2';
  frame.parsing = {recognized:['1','1','0','0','0','0'], numeric:'1', conflicting:'1'};
  parseFrame(JSON.stringify(frame));
  assert.match(parsingEvidence(frame).summary, /text 1 · JSON 1/);
  assert.match(parsingEvidence(frame).hint, /--level-field/);
  assert.match(parsingEvidence(frame).hint, /--profile pino/);
  for (const change of [p => p.recognized.push('0'), p => p.numeric = '3', p => p.recognized[0] = '3', p => p.conflicting = 1]) {
    const bad = structuredClone(frame); change(bad.parsing);
    assert.throws(() => parseFrame(JSON.stringify(bad)), /parsing/);
  }
  delete frame.parsing;
  assert.match(parsingEvidence(parseFrame(JSON.stringify(frame))).summary, /unavailable/);
});

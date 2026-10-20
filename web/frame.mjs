/** @typedef {{records: string, levels: Record<string, string>, parse_failures: string, oversized_records: string}} Counts */
/** @typedef {{configured_seconds: number, resolution_seconds: number, observed_seconds: number, counts: Counts, bytes_read: string, records_per_second: number, bytes_per_second: number, warning_ratio: number|null, error_ratio: number|null}} Window */
/** @typedef {{kind: string, session_id: string, source: string, state: string, elapsed_seconds: number, observed_at_unix_seconds: number, totals: Counts, bytes_read: string, pending_bytes: string, skipped_bytes: string, discarded_partial_records: string, backlog_bytes: string|null, warning_ratio: number|null, error_ratio: number|null, window: Window, history: string[]}} Metrics */
/** @typedef {{web_schema_version: 1, sequence: string, metrics: Metrics}} Frame */

export const levels = ['trace', 'debug', 'info', 'warn', 'error', 'fatal', 'unknown'];
export const finalStates = new Set(['complete', 'stopped', 'failed']);
const states = new Set(['reading', 'following', 'waiting', ...finalStates]);
const maxCounter = 18446744073709551615n;
const integerFormat = new Intl.NumberFormat('en-US');
const rateFormat = new Intl.NumberFormat('en-US', { maximumFractionDigits: 1 });
const percentFormat = new Intl.NumberFormat('en-US', { style: 'percent', maximumFractionDigits: 1 });

function requireField(valid) {
  if (!valid) throw new Error('Unsupported or invalid metrics frame.');
}
function counter(value) {
  requireField(typeof value === 'string' && /^(0|[1-9][0-9]{0,19})$/.test(value));
  requireField(BigInt(value) <= maxCounter);
}
function number(value) {
  requireField(typeof value === 'number' && Number.isFinite(value) && value >= 0);
}
function ratio(value) {
  if (value === null) return;
  number(value);
  requireField(value <= 1);
}
function counts(value) {
  counter(value.records);
  for (const level of levels) counter(value.levels[level]);
  counter(value.parse_failures);
  counter(value.oversized_records);
}

/** Validate the bounded production contract before any DOM updates.
 * @param {string} json
 * @returns {Frame}
 */
export function parseFrame(json) {
  requireField(json.length <= 65536 && new TextEncoder().encode(json).length <= 65536);
  try {
    const frame = JSON.parse(json);
    requireField(frame.web_schema_version === 1);
    counter(frame.sequence);
    const m = frame.metrics;
    requireField(typeof m.session_id === 'string' && m.session_id.length > 0 && !/[\r\n\0]/.test(m.session_id));
    requireField(typeof m.source === 'string' && states.has(m.state));
    requireField(m.kind === (finalStates.has(m.state) ? 'final' : 'snapshot'));
    number(m.elapsed_seconds);
    number(m.observed_at_unix_seconds);
    counts(m.totals);
    for (const key of ['bytes_read', 'pending_bytes', 'skipped_bytes', 'discarded_partial_records']) counter(m[key]);
    if (m.backlog_bytes !== null) counter(m.backlog_bytes);
    ratio(m.warning_ratio);
    ratio(m.error_ratio);
    const w = m.window;
    requireField(Number.isInteger(w.configured_seconds) && w.configured_seconds >= 1 && w.configured_seconds <= 3600);
    requireField(w.resolution_seconds === 0.1);
    number(w.observed_seconds);
    requireField(w.observed_seconds <= w.configured_seconds);
    counts(w.counts);
    counter(w.bytes_read);
    number(w.records_per_second);
    number(w.bytes_per_second);
    ratio(w.warning_ratio);
    ratio(w.error_ratio);
    requireField(Array.isArray(m.history) && m.history.length <= 60);
    for (const point of m.history) counter(point);
    return frame;
  } catch {
    throw new Error('Unsupported or invalid metrics frame.');
  }
}

/** Decimal strings stay exact through display, including u64::MAX. */
export function integer(value) { return integerFormat.format(BigInt(value)); }
export function rate(value) { return rateFormat.format(value); }
export function percent(value) { return value === null ? '—' : percentFormat.format(value); }
export function elapsed(seconds) {
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 3600).toString().padStart(2, '0')}:${Math.floor(whole / 60 % 60).toString().padStart(2, '0')}:${(whole % 60).toString().padStart(2, '0')}`;
}

/** Rebuild completed seconds from each full snapshot; never append per event.
 * Floating point is only used for SVG coordinates; labels retain exact strings.
 * @param {Metrics} metrics
 */
export function historyPoints(metrics) {
  const end = Math.floor(metrics.elapsed_seconds);
  return metrics.history.map((count, i) => ({ second: end - metrics.history.length + i, count, value: Number(count) }));
}

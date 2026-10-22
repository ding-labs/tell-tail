import { elapsed, finalStates, historyPoints, integer, levels, percent, rate } from './frame.mjs';

const byId = id => document.getElementById(id);
function text(id, value) { const el = byId(id); if (el.textContent !== value) el.textContent = value; }
const svgNS = 'http://www.w3.org/2000/svg';
let chartKey = '';
let chartSession = '';
let chartEnd = 0;
let chartVertices = [];
let chartTarget = [];
let chartAnimation = 0;
let chartInterpolationEnd = 0;
let chartTransform = null;
const chartMotion = byId('chart-motion');
// A small monitoring chart does not need to redraw at the display refresh rate.
// Keep the full-second transition, but bound SVG geometry updates to 20 Hz.
const chartFrameMs = 1000 / 20;
const chartLine = byId('chart-line');
const chartArea = byId('chart-area');
let chartDots = new Map();
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
let recentFrame = null;
let recentKey = '';
const fatalFilter = byId('fatal-only');
fatalFilter.addEventListener('click', () => {
  fatalFilter.setAttribute('aria-pressed', fatalFilter.getAttribute('aria-pressed') !== 'true' ? 'true' : 'false');
  if (recentFrame) recentErrors(recentFrame);
});

function recentErrors(frame) {
  recentFrame = frame;
  const fatalOnly = fatalFilter.getAttribute('aria-pressed') === 'true';
  const all = frame.recent_errors ?? [];
  const key = `${frame.metrics.session_id}:${fatalOnly}:${all.map(entry => entry.record).join(',')}`;
  if (key === recentKey) return;
  recentKey = key;
  const entries = all.filter(entry => !fatalOnly || entry.level === 'fatal');
  text('recent-errors-count', `${entries.length} retained`);
  text('recent-empty', fatalOnly ? 'No fatal records in the recent previews.' : 'No errors or fatal records captured.');
  byId('recent-empty').hidden = entries.length > 0;
  const list = byId('recent-errors');
  list.hidden = entries.length === 0;
  const rows = document.createDocumentFragment();
  for (const entry of entries) {
    const row = document.createElement('li');
    const meta = document.createElement('div');
    meta.className = 'recent-meta';
    const badge = document.createElement('span');
    badge.className = `recent-level recent-${entry.level}`;
    badge.textContent = entry.level;
    const position = document.createElement('span');
    position.textContent = `+${elapsed(entry.elapsed_seconds)} · entry ${integer(entry.record)}`;
    meta.append(badge, position);
    const preview = document.createElement('pre');
    preview.textContent = entry.preview;
    row.append(meta, preview);
    if (entry.truncated) {
      const note = document.createElement('span');
      note.className = 'subtle';
      note.textContent = 'Preview shortened…';
      row.append(note);
    }
    rows.append(row);
  }
  list.replaceChildren(rows);
}

// Monotone cubic interpolation rounds the line without overshooting counts.
function curve(vertices) {
  if (!vertices.length) return '';
  const slopes = vertices.slice(1).map((p, i) => (p.y - vertices[i].y) / (p.x - vertices[i].x));
  const tangents = vertices.map((_, i) => {
    if (!i) return slopes[0] ?? 0;
    if (i === vertices.length - 1) return slopes[i - 1];
    const a = slopes[i - 1], b = slopes[i];
    return a * b <= 0 ? 0 : 2 * a * b / (a + b);
  });
  let path = `M${vertices[0].x},${vertices[0].y}`;
  for (let i = 1; i < vertices.length; i++) {
    const a = vertices[i - 1], b = vertices[i], dx = (b.x - a.x) / 3;
    path += ` C${a.x + dx},${a.y + dx * tangents[i - 1]} ${b.x - dx},${b.y - dx * tangents[i]} ${b.x},${b.y}`;
  }
  return path;
}

function drawChart(vertices) {
  chartVertices = vertices;
  const path = curve(vertices);
  chartLine.setAttribute('d', path);
  chartArea.setAttribute('d', path ? `${path} L${vertices.at(-1).x},178 L${vertices[0].x},178 Z` : '');
  for (const p of vertices) {
    const dot = chartDots.get(p.second);
    if (dot) {
      dot.setAttribute('cx', p.x);
      dot.setAttribute('cy', p.y);
    }
  }
}

function cancelChartAnimation() {
  cancelAnimationFrame(chartAnimation);
  chartAnimation = 0;
  chartTransform?.cancel();
  chartTransform = null;
}

function visibleVertices() {
  // Publication can beat the last rAF of the preceding second. Finish within
  // one fallback paint interval instead of carrying tiny y-errors forever.
  if (chartAnimation && performance.now() >= chartInterpolationEnd - chartFrameMs) return chartTarget;
  if (!chartTransform) return chartVertices;
  const width = byId('chart').getBoundingClientRect().width;
  const shift = width ? new DOMMatrixReadOnly(getComputedStyle(chartMotion).transform).m41 * 760 / width : 0;
  return chartVertices.map(p => ({ ...p, x: p.x + shift }));
}

export function stopChartAnimation() {
  cancelChartAnimation();
  drawChart(chartTarget);
}
reducedMotion.addEventListener('change', () => { if (reducedMotion.matches) stopChartAnimation(); });
const severityRows = levels.map(level => {
  const row = document.createElement('tr');
  const name = document.createElement('th');
  name.scope = 'row';
  name.className = 'severity-name';
  const mark = document.createElement('span');
  mark.className = `severity-mark level-${level}`;
  mark.setAttribute('aria-hidden', 'true');
  name.append(mark, level);
  const count = document.createElement('td');
  const share = document.createElement('td');
  row.append(name, count, share);
  byId('severity').append(row);
  return { level, count, share };
});

/** @param {import('./frame.mjs').Metrics} m */
function chart(m) {
  const key = `${m.session_id}:${m.state}:${Math.floor(m.elapsed_seconds)}:${m.history.join(',')}`;
  if (key === chartKey) return;
  chartKey = key;
  const points = historyPoints(m);
  const max = Math.max(1, ...points.map(p => p.value));
  const vertices = points.map((p, i) => ({ ...p, x: 60 + (60 - points.length + i) / 59 * 688, y: 178 - p.value / max * 158 }));
  const end = Math.floor(m.elapsed_seconds);
  const animate = chartSession === m.session_id && end === chartEnd + 1 && chartVertices.length
    && vertices.length && !reducedMotion.matches && !document.hidden && !finalStates.has(m.state);
  const previous = visibleVertices();
  cancelChartAnimation();
  chartTarget = vertices;
  chartSession = m.session_id;
  chartEnd = end;
  const dots = document.createDocumentFragment();
  chartDots = new Map();
  const rows = document.createDocumentFragment();
  for (const p of vertices) {
    const interval = `${elapsed(p.second)}–${elapsed(p.second + 1)}`;
    const dot = document.createElementNS(svgNS, 'circle');
    for (const [key, value] of Object.entries({ cx: p.x, cy: p.y, r: 2.5 })) dot.setAttribute(key, String(value));
    dot.dataset.second = String(p.second);
    chartDots.set(p.second, dot);
    const title = document.createElementNS(svgNS, 'title');
    title.textContent = `${interval}: ${integer(p.count)} entries`;
    dot.append(title);
    dots.append(dot);
    const row = document.createElement('tr');
    for (const value of [interval, integer(p.count)]) {
      const cell = document.createElement('td');
      cell.textContent = value;
      row.append(cell);
    }
    rows.append(row);
  }
  byId('chart-dots').replaceChildren(dots);
  byId('chart-values').replaceChildren(rows);
  if (animate) {
    const step = 688 / 59;
    const previousBySecond = new Map(previous.map(p => [p.second, p]));
    const from = vertices.map(p => previousBySecond.get(p.second)
      ?? { ...p, x: p.x + step, y: previous.at(-1).y });
    // Keep neighbors at the clipped edge, but never accumulate invisible
    // history when publications repeatedly interrupt an unfinished scroll.
    const outgoing = previous.filter(p => p.second < vertices[0].second).slice(-2);
    const retained = vertices.filter(p => previousBySecond.has(p.second));
    const shift = retained.length ? previousBySecond.get(retained[0].second).x - retained[0].x : 0;
    const canTranslate = shift > 0 && shift <= step * 2 && retained.every(p => {
      const old = previousBySecond.get(p.second);
      return Math.abs(old.y - p.y) < 1e-6 && Math.abs(old.x - p.x - shift) < 1e-6;
    });
    if (canTranslate) {
      // Retained counts and scale are unchanged: rasterize once and let the
      // browser scroll a composited layer, without per-frame SVG/DOM writes.
      drawChart([...outgoing.map(p => ({ ...p, x: p.x - shift })), ...vertices]);
      const animation = chartMotion.animate([
        { transform: `translateX(${shift / 760 * 100}%)` },
        { transform: 'translateX(0%)' },
      ], { duration: 1000, easing: 'linear' });
      chartTransform = animation;
      animation.onfinish = () => {
        if (chartTransform !== animation) return;
        chartTransform = null;
        drawChart(chartTarget);
      };
    } else {
      // Rescaling or revised historical values still interpolate actual geometry.
      const start = performance.now();
      chartInterpolationEnd = start + 1000;
      let lastPaint = start;
      // Reuse interpolation objects instead of allocating a new graph every frame.
      const moving = [...outgoing, ...vertices].map(p => ({ ...p }));
      const tick = now => {
        const progress = Math.min(1, (now - start) / 1000);
        if (progress === 1) {
          chartAnimation = 0;
          drawChart(vertices);
          return;
        }
        if (now - lastPaint >= chartFrameMs) {
          lastPaint = now;
          for (let i = 0; i < outgoing.length; i++) moving[i].x = outgoing[i].x - step * progress;
          for (let i = 0; i < vertices.length; i++) {
            const p = moving[outgoing.length + i];
            p.x = from[i].x + (vertices[i].x - from[i].x) * progress;
            p.y = from[i].y + (vertices[i].y - from[i].y) * progress;
          }
          drawChart(moving);
        }
        chartAnimation = requestAnimationFrame(tick);
      };
      drawChart([...outgoing, ...from]);
      chartAnimation = requestAnimationFrame(tick);
    }
  } else drawChart(vertices);
  byId('chart-empty').setAttribute('visibility', points.length ? 'hidden' : 'visible');
  text('chart-max', points.length ? (max >= 1e6 ? max.toExponential(1) : rate(max)) : '0');
  text('chart-start', elapsed(Math.max(0, end - 60)));
  text('chart-end', elapsed(end));
  const caption = points.length ? `Latest completed second: ${integer(points.at(-1).count)} entries · ${points.length} of 60 seconds available` : 'No completed seconds yet.';
  text('chart-caption', caption);
  text('chart-description', caption);
}

/** @param {import('./frame.mjs').Frame} frame */
export function render(frame) {
  const m = frame.metrics;
  const w = m.window;
  for (const [id, value] of Object.entries({
    source: m.source, 'input-state': m.state, elapsed: elapsed(m.elapsed_seconds),
    'total-records': integer(m.totals.records), 'total-bytes': integer(m.bytes_read),
    'record-rate': rate(w.records_per_second), 'byte-rate': rate(w.bytes_per_second / 1048576),
    'warning-ratio': percent(w.warning_ratio), 'error-ratio': percent(w.error_ratio),
    'window-note': `${rate(w.observed_seconds)}s observed / ${w.configured_seconds}s configured${w.observed_seconds < w.configured_seconds ? ' · warming up' : ''}`,
    'parse-failures': integer(m.totals.parse_failures), oversized: integer(m.totals.oversized_records),
    skipped: integer(m.skipped_bytes), discarded: integer(m.discarded_partial_records),
    pending: integer(m.pending_bytes), backlog: m.backlog_bytes === null ? '—' : integer(m.backlog_bytes),
    'window-records': integer(w.counts.records), 'window-bytes': integer(w.bytes_read),
    'session-ratios': `Warning ${percent(m.warning_ratio)} · Error + fatal ${percent(m.error_ratio)}`,
  })) text(id, value);
  for (const row of severityRows) {
    const count = m.totals.levels[row.level];
    row.count.textContent = integer(count);
    row.share.textContent = m.totals.records === '0' ? '—' : percent(Number(count) / Number(m.totals.records));
  }
  chart(m);
  recentErrors(frame);
}

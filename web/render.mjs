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
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

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
  byId('chart-line').setAttribute('d', path);
  byId('chart-area').setAttribute('d', path ? `${path} L${vertices.at(-1).x},178 L${vertices[0].x},178 Z` : '');
  for (const dot of byId('chart-dots').children) {
    const p = vertices.find(p => p.second === Number(dot.dataset.second));
    if (p) {
      dot.setAttribute('cx', p.x);
      dot.setAttribute('cy', p.y);
    }
  }
}

export function stopChartAnimation() {
  cancelAnimationFrame(chartAnimation);
  chartAnimation = 0;
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
  cancelAnimationFrame(chartAnimation);
  chartAnimation = 0;
  const previous = chartVertices;
  chartTarget = vertices;
  chartSession = m.session_id;
  chartEnd = end;
  const dots = document.createDocumentFragment();
  const rows = document.createDocumentFragment();
  for (const p of vertices) {
    const interval = `${elapsed(p.second)}–${elapsed(p.second + 1)}`;
    const dot = document.createElementNS(svgNS, 'circle');
    for (const [key, value] of Object.entries({ cx: p.x, cy: p.y, r: 2.5 })) dot.setAttribute(key, String(value));
    dot.dataset.second = String(p.second);
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
    const from = vertices.map(p => previous.find(old => old.second === p.second)
      ?? { ...p, x: p.x + step, y: previous.at(-1).y });
    // Keep the outgoing point until it crosses the clipped left edge.
    const outgoing = previous.filter(p => p.second < vertices[0].second);
    const start = performance.now();
    const tick = now => {
      const progress = Math.min(1, (now - start) / 1000);
      const moving = vertices.map((p, i) => ({ ...p,
        x: from[i].x + (p.x - from[i].x) * progress,
        y: from[i].y + (p.y - from[i].y) * progress,
      }));
      drawChart([...outgoing.map(p => ({ ...p, x: p.x - step * progress })), ...moving]);
      if (progress < 1) chartAnimation = requestAnimationFrame(tick);
      else { chartAnimation = 0; drawChart(vertices); }
    };
    drawChart([...outgoing, ...from]);
    chartAnimation = requestAnimationFrame(tick);
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
}

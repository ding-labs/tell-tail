import { elapsed, finalStates, FrameError, parseFrame } from './frame.mjs';
import { render } from './render.mjs';

const connection = document.getElementById('connection');
const note = document.getElementById('display-note');
const freeze = document.getElementById('freeze');
const retry = document.getElementById('retry');
/** @type {import('./frame.mjs').Frame|null} */
let latest = null;
/** @type {import('./frame.mjs').Frame|null} */
let displayed = null;
let frozen = false;
let source = null;
let request = null;
let epoch = 0;
let animation = 0;
let retryTimer = 0;
let attempts = 0;
let lastAdvance = performance.now();
let phase = 'connecting';
let failureDetail = '';
const connectionWarnings = new Set();

// Report once per connection outage, without retaining or logging raw frames.
function warnConnection(reason, category = 'stream') {
  if (connectionWarnings.has(category)) return;
  connectionWarnings.add(category);
  console.warn(`[tell-tail] ${reason}; reconnecting. Last metrics retained.`);
}
function reportError(reason, endpoint = '/api/events') {
  console.error(`[tell-tail] ${reason}`, {
    endpoint,
    lastSequence: latest?.sequence ?? null,
    elapsedSeconds: latest?.metrics.elapsed_seconds ?? null,
  });
}

function setText(element, value) { if (element.textContent !== value) element.textContent = value; }
function isFinal() { return latest && finalStates.has(latest.metrics.state); }

// One latest frame and one scheduled paint, including while display is frozen.
function paint() {
  if (animation || document.hidden) return;
  animation = requestAnimationFrame(() => {
    animation = 0;
    if (latest && (!frozen || !displayed) && latest !== displayed) {
      displayed = latest;
      render(displayed);
    }
    let message;
    let tone = 'warning';
    if (isFinal()) {
      const state = latest.metrics.state;
      message = `${state === 'complete' ? 'Complete' : state === 'stopped' ? 'Stopped' : 'Input failed'} · final metrics retained`;
      tone = state === 'failed' ? 'error' : 'neutral';
    } else if (phase === 'live') {
      message = 'Live · connected to local monitor';
      tone = 'live';
    } else {
      message = {
        connecting: 'Connecting to local monitor…',
        reconnecting: 'Reconnecting · last metrics retained',
        stale: 'Stale · no new snapshot for at least 1 second. Reconnecting…',
        hidden: 'Updates paused while this tab is hidden',
        capacity: 'Stream capacity reached · four live tabs are already connected. Close one, then retry.',
        invalid: `${failureDetail} Last metrics retained. See browser console; retry after resolving the error.`,
        failure: 'Web output failed · last metrics retained',
      }[phase];
      if (phase === 'invalid' || phase === 'failure') tone = 'error';
    }
    setText(connection, message);
    connection.dataset.tone = tone;
    retry.hidden = isFinal() || !['capacity', 'invalid', 'failure', 'reconnecting', 'stale'].includes(phase);
    freeze.disabled = !displayed;
    freeze.setAttribute('aria-pressed', String(frozen));
    setText(freeze, frozen ? 'Resume display' : 'Freeze display');
    setText(note, frozen && displayed
      ? `Display frozen at ${elapsed(displayed.metrics.elapsed_seconds)}. ${isFinal() ? 'Final snapshot ready; resume to view it.' : 'Connection status stays live; resume to show the latest snapshot.'}`
      : displayed ? 'Showing the latest snapshot. Rolling values and session totals are labeled separately.' : 'Waiting for the first snapshot.');
  });
}

function retire() {
  epoch++;
  source?.close();
  source = null;
  request?.abort();
  request = null;
  clearTimeout(retryTimer);
  retryTimer = 0;
}

/** @param {import('./frame.mjs').Frame} frame */
function accept(frame, live) {
  const newer = !latest || frame.metrics.session_id !== latest.metrics.session_id || BigInt(frame.sequence) > BigInt(latest.sequence);
  if (newer) {
    latest = frame;
    lastAdvance = performance.now();
    if (live) {
      attempts = 0;
      if (connectionWarnings.size) console.info('[tell-tail] Connection recovered; receiving new metrics.');
      connectionWarnings.clear();
    }
    if (frame.metrics.state === 'failed') reportError('Input failed. Check the tell-tail terminal stderr for the cause.');
  }
  if (isFinal()) retire();
  else if (live) phase = performance.now() - lastAdvance < 1000 ? 'live' : 'stale';
  paint();
}

function invalid(error, endpoint) {
  failureDetail = error instanceof FrameError ? error.message : 'Unsupported or invalid metrics frame.';
  reportError(`${endpoint}: ${failureDetail}`, endpoint);
  retire();
  phase = 'invalid';
  paint();
}

async function recover(stale = false) {
  retire();
  warnConnection(stale ? 'No new snapshot for at least 1 second' : 'Event stream disconnected');
  phase = stale ? 'stale' : 'reconnecting';
  paint();
  const ticket = epoch;
  const controller = new AbortController();
  request = controller;
  const timeout = setTimeout(() => controller.abort(), 2000);
  try {
    const response = await fetch('/api/snapshot', { cache: 'no-store', signal: controller.signal });
    if (ticket !== epoch) return;
    if (response.ok) {
      const json = await response.text();
      if (ticket !== epoch) return;
      let frame;
      try { frame = parseFrame(json); }
      catch (error) { invalid(error, '/api/snapshot'); return; }
      accept(frame, false);
      if (isFinal()) return;
      if (response.headers.get('X-Tell-Tail-Streams-Available') === '0') {
        console.warn('[tell-tail] Stream capacity reached: four live tabs are connected. Close one and retry.');
        phase = 'capacity';
        paint();
        return;
      }
    } else {
      warnConnection(`Snapshot request returned HTTP ${response.status}`, 'snapshot');
    }
  } catch {
    if (ticket === epoch) warnConnection('Snapshot request failed or exceeded its 2 second deadline', 'snapshot');
  } finally {
    clearTimeout(timeout);
    if (request === controller) request = null;
  }
  if (ticket !== epoch || document.hidden) return;
  // Own retries instead of EventSource's opaque automatic reconnect loop.
  retryTimer = setTimeout(connect, Math.min(8000, 500 * 2 ** Math.min(attempts++, 4)));
}

function connect() {
  retire();
  if (document.hidden || isFinal()) { paint(); return; }
  failureDetail = '';
  phase = latest ? 'reconnecting' : 'connecting';
  paint();
  const stream = new EventSource('/api/events');
  source = stream;
  stream.addEventListener('snapshot', event => {
    if (source !== stream) return;
    try { accept(parseFrame(event.data), true); }
    catch (error) { invalid(error, '/api/events'); }
  });
  stream.addEventListener('failure', () => {
    if (source !== stream) return;
    reportError('Web publisher failed. Check the tell-tail terminal stderr for the cause.');
    retire();
    phase = 'failure';
    paint();
  });
  stream.onerror = () => { if (source === stream) void recover(); };
}

freeze.addEventListener('click', () => { frozen = !frozen; paint(); });
retry.addEventListener('click', () => { attempts = 0; connect(); });
document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    retire();
    cancelAnimationFrame(animation);
    animation = 0;
    if (!['capacity', 'invalid', 'failure'].includes(phase)) phase = 'hidden';
  } else if (!['capacity', 'invalid', 'failure'].includes(phase)) connect();
  paint();
});
window.addEventListener('pagehide', retire);
window.addEventListener('pageshow', event => { if (event.persisted) connect(); });
setInterval(() => {
  if (!document.hidden && source && !isFinal() && performance.now() - lastAdvance >= 1000) void recover(true);
}, 250);
connect();

// The exact same receive/decode/render path is used for both transports.
const elements = Object.fromEntries(['records', 'rate', 'sequence', 'history', 'transport']
  .map(id => [id, document.getElementById(id)]));
const sampleLimit = 50_000;
let connection, transport, pending, scheduled = false, frozen = false, lastSequence = 0n;
let samples = [], received = 0, rendered = 0, missed = 0, overflow = 0, errors = 0;
let calibration;

function render() {
  scheduled = false;
  if (frozen || !pending) return;
  const { frame, sample } = pending;
  pending = undefined;
  const began = performance.now();
  elements.records.textContent = BigInt(frame.metrics.totals.records).toString();
  elements.rate.textContent = frame.metrics.window.records_per_second.toFixed(1);
  elements.sequence.textContent = frame.sequence;
  const history = frame.metrics.history.map(Number);
  const max = Math.max(1, ...history);
  elements.history.setAttribute('points', history.map((n, i) => `${i * 600 / 59},${98 - n / max * 96}`).join(' '));
  sample.dom_ms = performance.now();
  sample.dom_work_ms = sample.dom_ms - began;
  rendered++;
}

function schedule() {
  if (!scheduled && !frozen) { scheduled = true; requestAnimationFrame(render); }
}

function receive(event) {
  const began = performance.now();
  const frame = JSON.parse(event.data);
  const decoded = performance.now();
  const sequence = BigInt(frame.sequence);
  if (sequence <= lastSequence) return;
  if (lastSequence !== 0n) missed += Number(sequence - lastSequence - 1n);
  lastSequence = sequence;
  const sample = { received_ms: began, published_us: frame.published_us, decode_ms: decoded - began };
  received++;
  if (samples.length < sampleLimit) samples.push(sample); else overflow++;
  pending = { frame, sample };
  schedule();
}

function disconnect() {
  connection?.close();
  connection = undefined;
  elements.transport.textContent = 'disconnected';
}

function connect(kind) {
  if (!['sse', 'websocket'].includes(kind)) throw new Error(`invalid transport ${kind}`);
  disconnect();
  transport = kind;
  elements.transport.textContent = kind;
  connection = kind === 'sse' ? new EventSource('/events') : new WebSocket(`${location.origin.replace('http:', 'ws:')}/ws`);
  if (kind === 'sse') connection.addEventListener('snapshot', receive);
  else connection.onmessage = receive;
  connection.onerror = () => { errors++; };
}

async function calibrate() {
  const probes = [];
  for (let i = 0; i < 12; i++) {
    const began = performance.now();
    const response = await fetch('/clock', { cache: 'no-store' });
    const value = await response.json();
    const ended = performance.now();
    probes.push({ rtt_ms: ended - began, offset_ms: (began + ended) / 2 - value.monotonic_us / 1000 });
  }
  // Midpoint mapping: uncertainty is at least half the chosen round-trip time.
  calibration = probes.reduce((a, b) => a.rtt_ms < b.rtt_ms ? a : b);
  return calibration;
}

window.bench = {
  connect, disconnect, calibrate,
  freeze(value) { frozen = value; if (!frozen) schedule(); },
  reset() { samples = []; received = rendered = missed = overflow = errors = 0; pending = undefined; },
  result() {
    return { samples, received, rendered, missed, overflow, errors, calibration,
      visibility: document.visibilityState, records: elements.records.textContent,
      sequence: elements.sequence.textContent,
      heap_bytes: performance.memory?.usedJSHeapSize ?? null };
  },
};

document.addEventListener('visibilitychange', () => {
  if (document.hidden) disconnect(); else if (transport) connect(transport);
});

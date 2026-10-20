import { finalStates, parseFrame } from './frame.mjs';
import { render } from './render.mjs';

const connection = document.getElementById('connection');
try {
  const response = await fetch('/api/snapshot', { cache: 'no-store', signal: AbortSignal.timeout(2000) });
  if (!response.ok) throw new Error('Could not load local metrics.');
  const frame = parseFrame(await response.text());
  render(frame);
  connection.textContent = finalStates.has(frame.metrics.state) ? `Input ${frame.metrics.state} · final snapshot` : 'Snapshot loaded';
  document.getElementById('display-note').textContent = 'Displaying the current snapshot.';
} catch (error) {
  connection.dataset.tone = 'error';
  connection.textContent = error.message;
}

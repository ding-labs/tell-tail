import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { createConnection } from 'node:net';
import { once } from 'node:events';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { startServer } from './server.mjs';

test('SSE and WebSocket deliver identical frames, current-state reconnect, and reject foreign origins', async () => {
  const server = await startServer(['--hz', '10']);
  const abort = new AbortController();
  let ws;
  try {
    assert.equal((await fetch(`${server.url}/status`, { headers: { origin: 'https://example.com' } })).status, 403);
    const response = await fetch(`${server.url}/events`, { signal: abort.signal });
    assert.equal(response.headers.get('content-type'), 'text/event-stream');
    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    const sse = new Map();
    const websocket = new Map();
    ws = new WebSocket(server.url.replace('http:', 'ws:') + '/ws');
    ws.onmessage = event => websocket.set(JSON.parse(event.data).sequence, event.data);
    let buffer = '';
    const deadline = performance.now() + 3000;
    while (![...sse.keys()].some(key => websocket.has(key))) {
      assert.ok(performance.now() < deadline, 'no matching frame');
      const part = await reader.read();
      assert.ok(!part.done);
      buffer += part.value;
      let boundary;
      while ((boundary = buffer.indexOf('\n\n')) !== -1) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        const data = event.split('\n').find(line => line.startsWith('data: ')).slice(6);
        sse.set(JSON.parse(data).sequence, data);
      }
    }
    const matched = [...sse.keys()].find(key => websocket.has(key));
    assert.equal(sse.get(matched), websocket.get(matched));
    abort.abort();
    ws.close();
    await delay(250);
    const reconnect = await fetch(`${server.url}/events`, { signal: AbortSignal.timeout(1000) });
    const resumed = await reconnect.body.pipeThrough(new TextDecoderStream()).getReader().read();
    assert.ok(Number(resumed.value.match(/id: (\d+)/)[1]) > Number(matched));
    // Timeout also releases this second stream even if the reader owns its lock.
  } finally {
    abort.abort();
    ws?.close();
    assert.ok(await server.stop() < 1000);
  }
});

for (const transport of ['sse', 'websocket']) test(`an undrained ${transport} socket preserves publication, other clients, and shutdown`, async () => {
  const server = await startServer(['--hz', '100', '--padding-bytes', '60000']);
  const url = new URL(server.url);
  const socket = createConnection({ host: url.hostname, port: Number(url.port) });
  try {
    await once(socket, 'connect');
    const upgrade = transport === 'websocket'
      ? 'Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n' : '';
    socket.write(`GET /${transport === 'sse' ? 'events' : 'ws'} HTTP/1.1\r\nHost: ${url.host}\r\n${upgrade}\r\n`);
    socket.pause();
    const first = await server.status();
    await delay(1000);
    if (process.platform === 'darwin') {
      const { stdout } = await promisify(execFile)('netstat', ['-an', '-p', 'tcp']);
      const own = stdout.split('\n').map(line => line.trim().split(/\s+/))
        .filter(fields => fields[3] === `127.0.0.1.${url.port}` && fields[5] === 'ESTABLISHED');
      assert.ok(own.some(fields => Number(fields[2]) > 0), 'verify a pending kernel send queue, not just a paused JS reader');
    }
    await delay(1500);
    const current = await server.status();
    assert.ok(current.sequence > first.sequence + 100);
    const active = await fetch(`${server.url}/events`, { signal: AbortSignal.timeout(1000) });
    const part = await active.body.getReader().read();
    assert.ok(part.value.byteLength > 0);
    assert.ok(await server.stop() < 1000);
  } finally {
    socket.destroy();
    if (server.child.exitCode === null && server.child.signalCode === null) await server.stop();
  }
});

test('stream admission is capped and a disconnected client releases capacity', async () => {
  const server = await startServer();
  const controllers = Array.from({ length: 4 }, () => new AbortController());
  try {
    for (const controller of controllers) {
      const response = await fetch(`${server.url}/events`, { signal: controller.signal });
      assert.equal(response.status, 200);
    }
    assert.equal((await server.status()).active_clients, 4);
    assert.equal((await fetch(`${server.url}/events`)).status, 429);
    controllers[0].abort();
    const deadline = performance.now() + 1000;
    while ((await server.status()).active_clients === 4 && performance.now() < deadline) await delay(20);
    controllers[0] = new AbortController();
    const recovered = await fetch(`${server.url}/events`, { signal: controllers[0].signal });
    assert.equal(recovered.status, 200);
  } finally { for (const controller of controllers) controller.abort(); await server.stop(); }
});

test('stdin counts come from ingestion and not the replay fixture', async () => {
  const server = await startServer(['--source', 'stdin']);
  try {
    assert.equal((await server.status()).frame.metrics.totals.records, '0');
    server.child.stdin.write('INFO one\nWARN two\n');
    const deadline = performance.now() + 2000;
    let status;
    do { await delay(20); status = await server.status(); }
    while (status.frame.metrics.totals.records !== '2' && performance.now() < deadline);
    assert.equal(status.frame.metrics.totals.records, '2');
    assert.equal(status.frame.metrics.totals.levels.warn, '1');
  } finally { await server.stop(); }
});

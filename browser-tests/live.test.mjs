import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { chromium, monitor } from './helpers.mjs';

const fixture = JSON.parse(await readFile(new URL('../tests/fixtures/output/empty-web-v1.json', import.meta.url), 'utf8'));
function frame(sequence, count, session = 'live') {
  const f = structuredClone(fixture);
  f.sequence = String(sequence);
  Object.assign(f.metrics, { kind: 'snapshot', state: 'reading', session_id: session });
  f.metrics.totals.records = String(count);
  f.metrics.totals.levels.info = String(count);
  return f;
}
async function setup(t, mock = false) {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const context = await browser.newContext();
  const page = await context.newPage();
  if (mock) await page.addInitScript(() => {
    window.streams = [];
    window.EventSource = class extends EventTarget {
      constructor() { super(); this.closed = false; window.streams.push(this); }
      close() { this.closed = true; }
    };
  });
  await page.goto(server.url);
  if (mock) await page.waitForFunction(() => window.streams.length > 0);
  return { server, page };
}
async function send(page, frames) {
  await page.evaluate(frames => {
    for (const f of frames) window.streams.at(-1).dispatchEvent(new MessageEvent('snapshot', { data: JSON.stringify(f) }));
  }, frames);
}
async function painted(page) { await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))); }

test('real SSE freezes only display, resumes by keyboard and retains final state', async t => {
  const { server, page } = await setup(t);
  const requests = [];
  page.on('request', r => { if (r.url().endsWith('/api/events')) requests.push(r.url()); });
  server.child.stdin.write('INFO first\n');
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '1');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Tab');
  assert.equal(await page.locator('#freeze').evaluate(el => el === document.activeElement), true);
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.getElementById('freeze').getAttribute('aria-pressed') === 'true');
  server.child.stdin.write('WARN second\nERROR third\n');
  // A real snapshot fetch confirms ingestion moved independently of the frozen UI.
  await page.waitForFunction(async () => (await (await fetch('/api/snapshot')).json()).metrics.totals.records === '3');
  assert.equal(await page.locator('#total-records').textContent(), '1');
  assert.match(await page.locator('#connection').textContent(), /Live/);
  await page.keyboard.press('Space');
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '3');
  await page.locator('#freeze').click();
  server.child.stdin.end('INFO final\n');
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Complete'));
  assert.equal(await page.locator('#total-records').textContent(), '3');
  assert.match(await page.locator('#display-note').textContent(), /Final snapshot ready/);
  await page.locator('#freeze').click();
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '4');
  const count = requests.length;
  await page.waitForTimeout(1250);
  assert.equal(requests.length, count, 'final state does not reconnect');
  await page.reload();
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '4');
  assert.match(await page.locator('#connection').textContent(), /Complete/);
});

test('generation zero, exact sequences, burst coalescing and session reset', async t => {
  const { page } = await setup(t, true);
  await send(page, [frame('0', '5')]);
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '5');
  await send(page, [frame('0', '99')]);
  await painted(page);
  assert.equal(await page.locator('#total-records').textContent(), '5');
  await page.evaluate(() => {
    window.changes = 0;
    new MutationObserver(records => window.changes += records.length).observe(document.getElementById('total-records'), { childList: true });
  });
  await send(page, Array.from({ length: 100 }, (_, i) => frame(i + 1, i + 6)));
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '105');
  assert.equal(await page.evaluate(() => window.changes), 1, 'one DOM update for a synchronous burst');
  await send(page, [frame('18446744073709551615', '123'), frame('18446744073709551614', '999')]);
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '123');
  const next = frame('0', '2', 'next-session');
  next.metrics.elapsed_seconds = 1.3;
  next.metrics.history = ['2'];
  await send(page, [next]);
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '2');
  assert.equal(await page.locator('#chart-values tr').count(), 1);
});

test('four streams have a clear capacity state, no automatic retries and manual recovery', async t => {
  const { server, page } = await setup(t);
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Live'));
  const held = await Promise.all(Array.from({ length: 3 }, () => fetch(`${server.url}api/events`)));
  t.after(() => Promise.all(held.map(r => r.body.cancel())));
  const fifth = await page.context().newPage();
  let eventRequests = 0;
  fifth.on('request', r => { if (r.url().endsWith('/api/events')) eventRequests++; });
  await fifth.goto(server.url);
  await fifth.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Stream capacity'));
  assert.equal(await fifth.locator('#total-records').textContent(), '0');
  await fifth.waitForTimeout(1500);
  assert.equal(eventRequests, 1);
  await held.pop().body.cancel();
  await fifth.locator('#retry').click();
  await fifth.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Live'));
  assert.equal(eventRequests, 2);
});

test('stale and disconnected metrics stay visible without claiming completion', async t => {
  const { page } = await setup(t, true);
  await page.route('**/api/snapshot', r => r.abort());
  await send(page, [frame(1, 7)]);
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '7');
  await page.locator('#freeze').click();
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Stale'));
  assert.equal(await page.evaluate(() => window.streams[0].closed), true);
  assert.equal(await page.locator('#total-records').textContent(), '7');
  assert.doesNotMatch(await page.locator('#connection').textContent(), /Complete|final metrics/);
  await page.waitForFunction(() => window.streams.length === 2);
  await send(page, [frame(2, 9)]);
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Live'));
  await page.locator('#freeze').click();
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '9');
});

test('an abruptly lost real server keeps frozen values and retries with backoff', async t => {
  const { server, page } = await setup(t);
  server.child.stdin.write('INFO retained\n');
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '1');
  await page.locator('#freeze').click();
  let attempts = 0;
  page.on('request', r => { if (r.url().endsWith('/api/events')) attempts++; });
  server.child.kill('SIGKILL');
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Reconnecting'));
  await page.waitForTimeout(2200);
  assert.equal(await page.locator('#total-records').textContent(), '1');
  assert.match(await page.locator('#connection').textContent(), /Reconnecting/);
  assert.equal(attempts >= 1 && attempts <= 3, true, `bounded retries: ${attempts}`);
});

test('invalid contracts and publisher failures stop streams and preserve last values while frozen', async t => {
  const { page } = await setup(t, true);
  await send(page, [frame(1, 8)]);
  await page.waitForFunction(() => document.getElementById('total-records').textContent === '8');
  await page.locator('#freeze').click();
  const bad = frame(2, 9);
  bad.web_schema_version = 2;
  await send(page, [bad]);
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Unsupported'));
  assert.equal(await page.evaluate(() => window.streams.at(-1).closed), true);
  assert.equal(await page.locator('#total-records').textContent(), '8');
  await page.locator('#retry').click();
  await page.evaluate(() => window.streams.at(-1).dispatchEvent(new MessageEvent('failure', { data: '{"message":"web output failed"}' })));
  await page.waitForFunction(() => document.getElementById('connection').textContent.startsWith('Web output failed'));
  assert.equal(await page.locator('#total-records').textContent(), '8');
  assert.equal(await page.evaluate(() => window.streams.at(-1).closed), true);
});

test('dashboard stays live across the full rolling window and history ring', async t => {
  const { page } = await setup(t, true);
  const errors = [];
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  for (const [index, seconds] of [59.999, 60, 60.001, 60.099, 60.1, 61.099, 120.099].entries()) {
    const next = frame(index, index + 1);
    next.metrics.elapsed_seconds = seconds;
    next.metrics.window.observed_seconds = seconds < 60 ? seconds : 60 + (Math.round(seconds * 1000) % 100) / 1000;
    next.metrics.history = Array(Math.min(60, Math.floor(seconds))).fill('1');
    await send(page, [next]);
    await page.waitForFunction(count => document.getElementById('total-records').textContent === String(count), index + 1);
    assert.match(await page.locator('#connection').textContent(), /^Live/);
    assert.equal(await page.evaluate(() => window.streams.at(-1).closed), false);
  }
  assert.deepEqual(errors, []);
});

import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { chromium, monitor } from './helpers.mjs';

const fixture = JSON.parse(await readFile(new URL('../tests/fixtures/output/empty-web-v1.json', import.meta.url), 'utf8'));

test('embedded dashboard renders real final metrics, diagnostics and responsive layout', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
  const requests = [];
  page.on('request', request => requests.push(request.url()));
  server.child.stdin.end('INFO ready\nWARN slow\nERROR failed\n{bad}\n');
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  assert.equal(await page.locator('#total-records').textContent(), '4');
  assert.equal(await page.locator('#warning-ratio').textContent(), '25%');
  assert.equal(await page.locator('#error-ratio').textContent(), '25%');
  await page.locator('.diagnostics summary').click();
  assert.equal(await page.locator('#parse-failures').textContent(), '1');
  assert.equal(await page.locator('#backlog').textContent(), '—');
  assert.match(await page.locator('#window-note').textContent(), /warming up/);
  for (const width of [1440, 768, 390, 320]) {
    await page.setViewportSize({ width, height: 1100 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `overflow at ${width}px`);
  }
  assert.equal(requests.every(url => url.startsWith(server.url)), true, 'all requests stay on loopback');
  assert.deepEqual(errors, []);
  assert.equal(server.stdout, '');
});

test('null values, u64 counters and untrusted sources render safely', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  const frame = structuredClone(fixture);
  frame.metrics.source = '<img src=x onerror="window.unsafe=true"> / 日誌.log';
  frame.metrics.elapsed_seconds = 62.9;
  frame.metrics.history = ['1', '2', '18446744073709551615'];
  frame.metrics.totals.records = '18446744073709551615';
  frame.metrics.totals.levels.info = frame.metrics.totals.records;
  await page.route('**/api/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: snapshot\ndata: ${JSON.stringify(frame)}\n\n` }));
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  assert.equal(await page.locator('#source').textContent(), frame.metrics.source);
  assert.equal(await page.locator('img').count(), 0);
  assert.equal(await page.evaluate(() => window.unsafe), undefined);
  assert.equal(await page.locator('#total-records').textContent(), '18,446,744,073,709,551,615');
  assert.equal(await page.locator('#warning-ratio').textContent(), '—');
  assert.equal(await page.locator('#record-rate').textContent(), '0');
  await page.locator('.chart-data summary').click();
  assert.equal(await page.locator('#chart-values tr').count(), 3);
  assert.match(await page.locator('#chart-values').textContent(), /18,446,744,073,709,551,615/);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});

test('chart curves stay bounded and animate between seconds with motion controls', async t => {
  const server = await monitor();
  t.after(() => server.close());
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.route('**/api/events', route => route.fulfill({ contentType: 'text/event-stream', body: `event: snapshot\ndata: ${JSON.stringify(fixture)}\n\n` }));
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('input-state').textContent === 'complete');
  await page.clock.install();
  await page.clock.pauseAt(new Date());
  const update = async (end, history, session = 'motion', state = 'reading') => page.evaluate(async ({ fixture, end, history, session, state }) => {
    const { render } = await import('/render.mjs');
    const frame = structuredClone(fixture);
    Object.assign(frame.metrics, { elapsed_seconds: end, history, session_id: session, state });
    render(frame);
  }, { fixture, end, history, session, state });
  const path = () => page.locator('#chart-line').getAttribute('d');
  await update(60, Array.from({ length: 60 }, (_, i) => i % 2 ? '100' : '0'));
  assert.match(await path(), / C/);
  const bounds = await page.locator('#chart-line').evaluate(el => {
    const length = el.getTotalLength();
    const ys = Array.from({ length: 1001 }, (_, i) => el.getPointAtLength(length * i / 1000).y);
    return [Math.min(...ys), Math.max(...ys)];
  });
  assert.ok(bounds[0] >= 19.99 && bounds[1] <= 178.01, 'curve never invents negative counts or overshoots peaks');
  await page.evaluate(() => {
    window.chartPaints = 0;
    window.chartObserver = new MutationObserver(records => {
      window.chartPaints += records.filter(record => record.attributeName === 'd').length;
    });
    window.chartObserver.observe(document.getElementById('chart-line'), { attributes: true });
  });
  await update(61, Array.from({ length: 60 }, (_, i) => i % 2 ? '0' : '100'));
  const before = await path();
  await page.clock.runFor(250);
  const during = await path();
  await page.clock.runFor(250);
  assert.notEqual(await path(), during, 'motion continues between incoming samples');
  assert.notEqual(before, during);
  await page.clock.runFor(550);
  const paints = await page.evaluate(() => {
    window.chartObserver.disconnect();
    return window.chartPaints;
  });
  assert.ok(paints >= 10 && paints <= 22, `full-second animation has bounded SVG updates: ${paints}`);
  assert.equal(await page.locator('#chart-values tr').count(), 60);
  await update(62, Array(60).fill('25'));
  await page.clock.runFor(100);
  await page.locator('#freeze').click();
  const frozen = await path();
  await page.clock.runFor(1200);
  assert.equal(await path(), frozen, 'freeze cancels ongoing motion');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await update(63, Array(60).fill('0'));
  const reduced = await path();
  await page.clock.runFor(500);
  assert.equal(await path(), reduced, 'reduced motion renders immediately');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await update(64, ['2', '8', '1'], 'new-session');
  const reset = await path();
  await page.clock.runFor(500);
  assert.equal(await path(), reset, 'session resets do not interpolate unrelated history');
  await update(65, ['8', '1', '4'], 'new-session', 'complete');
  const final = await path();
  await page.clock.runFor(1200);
  assert.equal(await path(), final, 'final metrics render immediately');
});

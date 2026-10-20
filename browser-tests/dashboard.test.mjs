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
  await page.route('**/api/snapshot', route => route.fulfill({ json: frame }));
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

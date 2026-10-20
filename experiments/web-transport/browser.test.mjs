import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, launchOptions } from './playwright.mjs';
import { startServer } from './server.mjs';
import { launchDesktop } from './desktop-browser.mjs';

test('both browser clients preserve exact counters, freeze, and resume current state', { timeout: 20_000 }, async () => {
  const server = await startServer(['--profile', 'large-counters', '--hz', '100']);
  const browser = await chromium.launch(launchOptions);
  try {
    const page = await browser.newPage();
    await page.goto(server.url);
    for (const transport of ['sse', 'websocket']) {
      await page.evaluate(async kind => { await window.bench.calibrate(); window.bench.reset(); window.bench.connect(kind); }, transport);
      await page.waitForFunction(() => window.bench.result().rendered >= 5);
      assert.equal(await page.locator('#records').textContent(), '18446744073709551615');
      await page.evaluate(() => window.bench.freeze(true));
      const frozen = await page.locator('#sequence').textContent();
      const before = await page.evaluate(() => window.bench.result().received);
      await delay(150);
      assert.equal(await page.locator('#sequence').textContent(), frozen);
      assert.ok(await page.evaluate(() => window.bench.result().received) > before);
      await page.evaluate(() => window.bench.freeze(false));
      await page.waitForFunction(sequence => BigInt(document.getElementById('sequence').textContent) > BigInt(sequence), frozen);
      await page.evaluate(() => window.bench.disconnect());
      await delay(100);
      await page.evaluate(kind => { window.bench.reset(); window.bench.connect(kind); }, transport);
      await page.waitForFunction(sequence => BigInt(document.getElementById('sequence').textContent) > BigInt(sequence), frozen);
      const result = await page.evaluate(() => window.bench.result());
      assert.equal(result.overflow, 0);
      assert.equal(result.errors, 0);
      assert.ok(result.calibration.rtt_ms >= 0);
      await page.evaluate(() => window.bench.disconnect());
    }
  } finally { await browser.close(); await server.stop(); }
});

test('a genuinely hidden desktop tab closes its stream and resumes current state', {
  timeout: 20_000, skip: process.env.RUN_HEADED_TESTS !== '1',
}, async () => {
  const server = await startServer();
  const desktop = await launchDesktop();
  try {
    const context = desktop.browser.contexts()[0];
    const page = context.pages()[0];
    page.setDefaultTimeout(5000);
    await page.goto(server.url);
    const other = await context.newPage();
    for (const transport of ['sse', 'websocket']) {
      await page.bringToFront();
      await page.evaluate(kind => { window.bench.reset(); window.bench.connect(kind); }, transport);
      await page.waitForFunction(() => window.bench.result().rendered > 0);
      const before = await page.locator('#sequence').textContent();
      await other.bringToFront();
      await page.waitForFunction(() => document.hidden, null, { polling: 50 });
      const deadline = performance.now() + 2000;
      while ((await server.status()).active_clients !== 0 && performance.now() < deadline) await delay(20);
      assert.equal((await server.status()).active_clients, 0);
      await delay(150);
      await page.bringToFront();
      await page.waitForFunction(sequence => !document.hidden && BigInt(document.getElementById('sequence').textContent) > BigInt(sequence), before);
      assert.equal((await server.status()).active_clients, 1);
      await page.evaluate(() => window.bench.disconnect());
    }
  } finally { await desktop.close(); await server.stop(); }
});

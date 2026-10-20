import assert from 'node:assert/strict';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium, launchOptions } from './playwright.mjs';
import { startServer } from './server.mjs';

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
      await page.evaluate(kind => window.bench.connect(kind), transport);
      await page.waitForFunction(sequence => BigInt(document.getElementById('sequence').textContent) > BigInt(sequence), frozen);
      const result = await page.evaluate(() => window.bench.result());
      assert.equal(result.overflow, 0);
      assert.equal(result.errors, 0);
      assert.ok(result.calibration.rtt_ms >= 0);
      await page.evaluate(() => window.bench.disconnect());
    }
  } finally { await browser.close(); await server.stop(); }
});

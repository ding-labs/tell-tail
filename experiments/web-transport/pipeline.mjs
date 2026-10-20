// Validation of actual input -> engine publication -> sampled transport -> DOM.
// Host monotonic timing includes producer writes and Playwright observation cost.
import assert from 'node:assert/strict';
import { writeSync, closeSync, openSync } from 'node:fs';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';
import { chromium, launchOptions } from './playwright.mjs';
import { startServer } from './server.mjs';
import { distribution } from './measurements.mjs';

const { values } = parseArgs({ options: {
  samples: { type: 'string', default: '25' },
  output: { type: 'string', default: fileURLToPath(new URL('./results/pipeline.json', import.meta.url)) },
} });
const count = Number(values.samples);
assert.ok(Number.isInteger(count) && count >= 5 && count <= 100);
const browser = await chromium.launch(launchOptions);
const directory = await mkdtemp(join(tmpdir(), 'tell-tail-web-pipeline-'));
const results = [];

try {
  for (const source of ['stdin', 'file']) for (const load of ['idle', 'paced']) for (const transport of ['sse', 'websocket']) {
    const path = join(directory, 'input.log');
    await writeFile(path, '');
    const fd = source === 'file' ? openSync(path, 'a') : null;
    const server = await startServer(['--source', source, ...(source === 'file' ? ['--input-file', path] : [])]);
    const page = await browser.newPage();
    let timer;
    try {
      await page.goto(server.url);
      await page.evaluate(kind => window.bench.connect(kind), transport);
      await page.waitForFunction(() => document.getElementById('records').textContent === '0');
      const observation = [];
      for (let i = 0; i < 12; i++) {
        const began = performance.now();
        await page.evaluate(() => document.getElementById('records').textContent);
        observation.push(performance.now() - began);
      }
      let produced = 0;
      let bytes = 0;
      const write = (chunk, records) => {
        if (source === 'stdin' && server.child.stdin.writableLength > 64 * 1024) return false;
        bytes += chunk.length;
        assert.ok(bytes < 64 * 1024 * 1024, 'pipeline fixture exceeds 64 MiB');
        if (fd !== null) {
          // Handle short writes rather than assuming the fixture reached the file.
          let offset = 0;
          while (offset < chunk.length) offset += writeSync(fd, chunk, offset);
        } else server.child.stdin.write(chunk);
        produced += records;
        return true;
      };
      if (load === 'paced') {
        const batch = Buffer.from('INFO paced\n'.repeat(1000));
        timer = setInterval(() => write(batch, 1000), 50); // Nominal 20K records/s, not saturation.
      }
      const latencies = [];
      const began = performance.now();
      for (let i = 0; i < count; i++) {
        await delay(7 + (i * 17 % 31)); // Vary phase relative to publication ticks.
        const started = performance.now();
        assert.ok(write(Buffer.from('WARN marker\n'), 1), 'marker injection was backpressured; trial is invalid');
        const target = String(produced);
        await page.waitForFunction(expected => BigInt(document.getElementById('records').textContent) >= BigInt(expected), target, { timeout: 3000 });
        latencies.push(performance.now() - started);
      }
      clearInterval(timer);
      await page.waitForFunction(expected => document.getElementById('records').textContent === expected, String(produced), { timeout: 3000 });
      const seconds = (performance.now() - began) / 1000;
      const result = { source, load, transport, samples: count,
        write_to_dom_ms: distribution(latencies), automation_round_trip_ms: distribution(observation),
        produced, bytes, seconds, observed_records_per_second: produced / seconds,
      };
      result.shutdown_ms = await server.stop();
      results.push(result);
      console.error(`${source} ${load} ${transport}: write-to-DOM p95 ${result.write_to_dom_ms.p95.toFixed(2)} ms, ${produced} exact records`);
      await mkdir(dirname(resolve(values.output)), { recursive: true });
      await writeFile(values.output, JSON.stringify({ schema_version: 1, browser: browser.version(), results }, null, 2) + '\n');
    } finally {
      clearInterval(timer);
      if (fd !== null) closeSync(fd);
      await page.close();
      if (server.child.exitCode === null && server.child.signalCode === null) await server.stop();
    }
  }
} finally { await browser.close(); await rm(directory, { recursive: true, force: true }); }

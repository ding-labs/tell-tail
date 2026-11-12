// Standalone acceptance measurement, not a timing assertion in shared CI.
// Build release first: TELL_TAIL_BIN=target/release/tell-tail node browser-tests/mcp-benchmark.mjs
import { mkdtemp, writeFile, appendFile, rm } from 'node:fs/promises';
import { tmpdir, platform, arch, cpus } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { chromium, monitorMcp } from './helpers.mjs';
const exec = promisify(execFile);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const line = name => `${new Date().toISOString()} info [AppServerConnection] mcp_server_startup_status_updated server=${name} status=failed failureReason=reauthenticationRequired threadId=synthetic\n`;
const cpuSeconds = value => value.split(':').reduce((total, part) => total * 60 + Number(part), 0);
async function usage(pid) {
  const { stdout } = await exec('ps', ['-o', 'rss=,time=', '-p', String(pid)]);
  const [rss, time] = stdout.trim().split(/\s+/);
  return { rssBytes: Number(rss) * 1024, cpuSeconds: cpuSeconds(time) };
}
const dir = await mkdtemp(join(tmpdir(), 'tell-tail-mcp-benchmark-'));
let server, browser;
try {
  for (let n = 0; n < 32; n++) {
    const event = line(`fixture-${n}`), bytes = 512 * 1024 - Buffer.byteLength(event);
    const filler = ('application noise\n'.repeat(Math.ceil(bytes / 18))).slice(0, bytes - 1) + '\n';
    await writeFile(join(dir, `${n}.log`), filler + event);
  }
  browser = await chromium.launch(); const page = await browser.newPage();
  const started = performance.now();
  server = await monitorMcp(['--source', 'codex-desktop', '--log-dir', dir]);
  await page.goto(server.url);
  await page.waitForFunction(() => document.getElementById('event-count').textContent === '32' && document.querySelectorAll('#timeline tr').length === 32);
  const firstViewMs = performance.now() - started;
  const latencyMs = [];
  for (let n = 0; n < 30; n++) {
    const began = performance.now();
    await appendFile(join(dir, '0.log'), line(`latency-${n}`));
    await page.waitForFunction(name => [...document.querySelectorAll('#timeline tr')].some(row => row.textContent.includes(name)), `latency-${n}`);
    latencyMs.push(performance.now() - began);
    await sleep(37 + n % 7 * 13);
  }
  const newAt = performance.now();
  await writeFile(join(dir, 'discovered.log'), line('new-file'));
  await page.waitForFunction(() => document.getElementById('timeline').textContent.includes('new-file'));
  const discoveryMs = performance.now() - newAt;
  await sleep(500);
  const startUsage = await usage(server.child.pid), idleAt = performance.now();
  await sleep(10000);
  const endUsage = await usage(server.child.pid), idleSeconds = (performance.now() - idleAt) / 1000;
  const snapshot = await page.evaluate(async () => (await (await fetch('/api/mcp/snapshot')).json()));
  const stopAt = performance.now(); await server.close();
  const shutdownMs = performance.now() - stopAt;
  const p95 = [...latencyMs].sort((a, b) => a - b)[Math.ceil(latencyMs.length * .95) - 1];
  const result = {
    measured_at: new Date().toISOString(), binary: process.env.TELL_TAIL_BIN || 'target/debug/tell-tail',
    platform: platform(), architecture: arch(), processor: cpus()[0]?.model,
    workload: { files: 32, history_bytes: 16 * 1024 * 1024, live_samples: latencyMs.length, idle_seconds: idleSeconds },
    first_usable_browser_ms: firstViewMs, append_to_browser_p95_ms: p95, append_to_browser_samples_ms: latencyMs,
    new_file_to_browser_ms: discoveryMs, idle_cpu_percent_one_core: (endUsage.cpuSeconds - startUsage.cpuSeconds) / idleSeconds * 100,
    rss_bytes: endUsage.rssBytes, shutdown_ms: shutdownMs, forced_kill: server.child.signalCode === 'SIGKILL',
    observations: snapshot.counts.observations, sources: snapshot.sources.length,
    limits: { first_view_ms: 2000, append_p95_ms: 500, discovery_ms: 2000, idle_cpu_percent: 1, rss_bytes: 128 * 1024 * 1024, shutdown_ms: 1000 },
  };
  result.targets_met = firstViewMs < 2000 && p95 < 500 && discoveryMs < 2000 && result.idle_cpu_percent_one_core < 1 && result.rss_bytes < result.limits.rss_bytes && shutdownMs < 1000 && !result.forced_kill;
  console.log(JSON.stringify(result, null, 2));
  if (!result.targets_met) process.exitCode = 1;
} finally { await server?.close(); await browser?.close(); await rm(dir, { recursive: true, force: true }); }

import { execFile } from 'node:child_process';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { availableParallelism, cpus, loadavg, platform, release } from 'node:os';
import { resolve, dirname } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { parseArgs, promisify } from 'node:util';
import { chromium, launchOptions } from './playwright.mjs';
import { startServer } from './server.mjs';
import { analyze, distribution, parseProcesses, processTree, resources } from './measurements.mjs';

const exec = promisify(execFile);
const { values } = parseArgs({ options: {
  hz: { type: 'string', default: '10,100' }, clients: { type: 'string', default: '0,1,4' },
  profiles: { type: 'string', default: 'representative,large-counters' },
  pairs: { type: 'string', default: '5' }, seconds: { type: 'string', default: '10' },
  warmup: { type: 'string', default: '1' }, output: { type: 'string', default: 'experiments/web-transport/results/benchmark.json' },
} });
const numeric = (text, min, max, integer = true) => {
  const n = Number(text);
  if (!Number.isFinite(n) || n < min || n > max || (integer && !Number.isInteger(n))) throw new Error(`invalid numeric option ${text}`);
  return n;
};
const config = {
  hz: values.hz.split(',').map(x => numeric(x, 1, 100)), clients: values.clients.split(',').map(x => numeric(x, 0, 4)),
  profiles: values.profiles.split(','), pairs: numeric(values.pairs, 1, 20),
  seconds: numeric(values.seconds, .1, 120, false), warmup: numeric(values.warmup, 0, 30, false),
};
if (config.profiles.some(p => !['representative', 'large-counters'].includes(p))) throw new Error('invalid profile');
const report = { schema_version: 1, config, host: {
  platform: platform(), release: release(), cpu: cpus()[0].model, logical_cpus: availableParallelism(),
  load_at_start: loadavg(), node: process.version, browser: null,
  commit: (await exec('git', ['rev-parse', 'HEAD'])).stdout.trim(),
  dirty: Boolean((await exec('git', ['status', '--porcelain'])).stdout.trim()),
  rust: (await exec('rustc', ['--version'])).stdout.trim(),
  playwright: JSON.parse(await readFile(new URL('./package.json', import.meta.url))).devDependencies.playwright,
}, runs: [] };

const browserServer = await chromium.launchServer(launchOptions);
const browser = await chromium.connect(browserServer.wsEndpoint());
report.host.browser = browser.version();
report.host.browser_args = launchOptions.args;

async function snapshot(serverPid) {
  const processes = parseProcesses((await exec('ps', ['-axo', 'pid=,ppid=,rss=,time='])).stdout);
  return { server: processTree(processes, serverPid), browser: processTree(processes, browserServer.process().pid) };
}

async function run(hz, clients, profile, transport, pair) {
  const server = await startServer(['--hz', String(hz), '--profile', profile]);
  const context = await browser.newContext({ viewport: { width: 900, height: 600 } });
  let timer;
  try {
    const pages = [];
    const pageErrors = [];
    for (let i = 0; i < clients; i++) {
      const page = await context.newPage();
      page.on('pageerror', error => { pageErrors.push(error.message); });
      await page.goto(server.url);
      await page.evaluate(async kind => { await window.bench.calibrate(); window.bench.connect(kind); }, transport);
      await page.waitForFunction(() => window.bench.result().rendered >= 2);
      pages.push(page);
    }
    await delay(config.warmup * 1000);
    const status = await server.status();
    if (status.active_clients !== clients) throw new Error('client admission mismatch');
    const observations = [await snapshot(server.pid)];
    await Promise.all(pages.map(page => page.evaluate(() => window.bench.reset())));
    const began = performance.now();
    let sampling = false;
    let samplingError;
    timer = setInterval(async () => {
      if (sampling) return;
      sampling = true;
      try { observations.push(await snapshot(server.pid)); } catch (error) { samplingError = error; }
      finally { sampling = false; }
    }, 1000);
    await delay(config.seconds * 1000);
    clearInterval(timer);
    while (sampling) await delay(5);
    if (samplingError) throw samplingError;
    const after = await snapshot(server.pid);
    const elapsed = (performance.now() - began) / 1000;
    observations.push(after);
    const raw = await Promise.all(pages.map(page => page.evaluate(() => { window.bench.disconnect(); return window.bench.result(); })));
    if (pageErrors.length) throw new Error(`browser exceptions: ${pageErrors.join('; ')}`);
    const endClocks = await Promise.all(pages.map(page => page.evaluate(() => window.bench.calibrate())));
    const measured = raw.map((client, i) => analyze(client, endClocks[i]));
    const final = await server.status();
    const result = {
      hz, clients, profile, transport, pair, seconds: elapsed, payload_bytes: status.payload_bytes,
      published: final.sequence - status.sequence,
      server: resources(observations[0].server, after.server, observations.map(p => p.server)),
      browser: resources(observations[0].browser, after.browser, observations.map(p => p.browser)),
      clients_measured: measured, raw_clients: raw,
    };
    result.shutdown_ms = await server.stop();
    console.error(`${hz}Hz ${profile} ${clients} client(s) pair ${pair + 1} ${transport}: ` +
      `receipt p95 ${measured.map(c => c.receipt_ms.p95.toFixed(3)).join('/') || 'n/a'} ms; DOM p95 ${measured.map(c => c.dom_ms?.p95.toFixed(2)).join('/') || 'n/a'} ms`);
    return result;
  } finally {
    clearInterval(timer);
    await context.close();
    if (server.child.exitCode === null && server.child.signalCode === null) await server.stop();
  }
}

try {
  for (const hz of config.hz) for (const profile of config.profiles) for (const clients of config.clients) {
    for (let pair = 0; pair < config.pairs; pair++) {
      const order = pair % 2 ? ['websocket', 'sse'] : ['sse', 'websocket'];
      for (const transport of order) {
        report.runs.push(await run(hz, clients, profile, transport, pair));
        await mkdir(dirname(resolve(values.output)), { recursive: true });
        await writeFile(values.output, JSON.stringify(report) + '\n');
      }
    }
  }
  const groups = [];
  for (const hz of config.hz) for (const profile of config.profiles) for (const clients of config.clients) for (const transport of ['sse', 'websocket']) {
    const runs = report.runs.filter(r => r.hz === hz && r.profile === profile && r.clients === clients && r.transport === transport);
    const measured = runs.flatMap(r => r.clients_measured);
    groups.push({ hz, profile, clients, transport, runs: runs.length, payload_bytes: distribution(runs.map(r => r.payload_bytes)),
      receipt_p95_ms: distribution(measured.map(c => c.receipt_ms.p95)), dom_p95_ms: distribution(measured.map(c => c.dom_ms.p95)),
      receipt_p99_ms: distribution(measured.map(c => c.receipt_ms.p99)), dom_p99_ms: distribution(measured.map(c => c.dom_ms.p99)),
      max_clock_uncertainty_ms: measured.length ? Math.max(...measured.map(c => c.clock_uncertainty_ms)) : null,
      max_clock_offset_change_ms: measured.length ? Math.max(...measured.map(c => Math.abs(c.clock_offset_change_ms))) : null,
      received: measured.reduce((n, c) => n + c.received, 0), rendered: measured.reduce((n, c) => n + c.rendered, 0),
      missed: measured.reduce((n, c) => n + c.missed, 0),
      server_cpu_percent: distribution(runs.map(r => r.server.cpu_seconds / r.seconds * 100)),
      server_rss_peak_kib: distribution(runs.map(r => r.server.rss_peak_kib)),
      browser_cpu_percent: distribution(runs.map(r => r.browser.cpu_seconds / r.seconds * 100)),
      shutdown_ms: distribution(runs.map(r => r.shutdown_ms)),
    });
  }
  await writeFile(values.output.replace(/\.json$/, '') + '-summary.json', JSON.stringify({ schema_version: 1, host: report.host, config, groups }, null, 2) + '\n');
} finally { await browser.close(); await browserServer.close(); }

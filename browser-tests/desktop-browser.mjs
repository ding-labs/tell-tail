import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from './helpers.mjs';

// Playwright normally emulates page focus, making background pages visible.
// A fresh disposable browser + noDefaults preserves native tab visibility.
export async function launchDesktop() {
  const profile = await mkdtemp(join(tmpdir(), 'tell-tail-visibility-'));
  const child = spawn(chromium.executablePath(), [
    // Match Playwright's test launcher on Linux runners with restricted userns.
    ...(process.platform === 'linux' ? ['--no-sandbox'] : []),
    '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1',
    `--user-data-dir=${profile}`, '--disable-background-networking',
    '--disable-backgrounding-occluded-windows', '--no-first-run',
    '--no-default-browser-check', 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });
  let browser;
  let spawnError;
  let startupDiagnostics = '';
  child.stderr.on('data', chunk => {
    startupDiagnostics = (startupDiagnostics + chunk.toString()).slice(-8192);
  });
  child.once('error', error => { spawnError = error; });
  async function close() {
    await browser?.close();
    if (!spawnError && child.exitCode === null && child.signalCode === null) {
      const exited = once(child, 'exit');
      child.kill('SIGTERM');
      const timer = setTimeout(() => child.kill('SIGKILL'), 1000);
      await exited;
      clearTimeout(timer);
    }
    await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
  try {
    let port;
    // Cold browser startup can exceed five seconds on shared CI runners.
    const deadline = Date.now() + 15_000;
    while (Date.now() < deadline) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null || child.signalCode !== null) {
        throw new Error(`desktop browser exited during startup: ${startupDiagnostics}`);
      }
      try {
        const candidate = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
        if (/^\d+$/.test(candidate) && Number(candidate) > 0 && Number(candidate) <= 65535) {
          port = candidate;
          break;
        }
      } catch { /* The browser creates the file after initialization. */ }
      await delay(50);
    }
    if (!port) throw new Error(`desktop browser did not expose its local debugging port within 15s: ${startupDiagnostics}`);
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { noDefaults: true });
    return { browser, close };
  } catch (error) { await close(); throw error; }
}

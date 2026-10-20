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
  ], { stdio: 'ignore' });
  let browser;
  let spawnError;
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
    for (let i = 0; i < 100; i++) {
      if (spawnError) throw spawnError;
      if (child.exitCode !== null || child.signalCode !== null) throw new Error('desktop browser exited during startup');
      try { port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]; break; }
      catch { await delay(50); }
    }
    if (!port) throw new Error('desktop browser did not expose its local debugging port');
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { noDefaults: true });
    return { browser, close };
  } catch (error) { await close(); throw error; }
}

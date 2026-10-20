import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';

const modulePath = process.env.PLAYWRIGHT_MODULE_DIR
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_DIR, 'index.mjs')).href : 'playwright';
export const { chromium } = await import(modulePath);
export const root = fileURLToPath(new URL('..', import.meta.url));

export async function monitor(args = []) {
  const child = spawn(resolve(root, process.env.TELL_TAIL_BIN || 'target/debug/tell-tail'), ['--output', 'web', ...args], { cwd: root, stdio: ['pipe', 'pipe', 'pipe'] });
  let diagnostic = '';
  let stdout = '';
  child.stdout.on('data', data => { stdout = (stdout + data).slice(-16384); });
  child.stdin.on('error', () => {});
  const url = await new Promise((accept, reject) => {
    const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error(`Server startup timed out: ${diagnostic}`)); }, 3000);
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Server exited ${code}: ${diagnostic}`)); });
    child.stderr.on('data', data => {
      diagnostic = (diagnostic + data).slice(-16384);
      const match = diagnostic.match(/http:\/\/127\.0\.0\.1:\d+\//);
      if (match) { clearTimeout(timer); accept(match[0]); }
    });
  });
  return {
    child, url,
    get stdout() { return stdout; },
    async close() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      const exited = once(child, 'exit');
      child.kill('SIGTERM');
      const timer = setTimeout(() => child.kill('SIGKILL'), 1000);
      await exited;
      clearTimeout(timer);
    },
  };
}

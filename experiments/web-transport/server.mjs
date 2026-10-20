import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

export const defaultBinary = fileURLToPath(new URL('./target/release/tell-tail-web-transport', import.meta.url));

export async function startServer(args = [], binary = defaultBinary) {
  const child = spawn(binary, args, { stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-16_384); });
  const lines = createInterface({ input: child.stdout });
  const ready = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('server startup timed out')); }, 10_000);
    lines.once('line', line => {
      clearTimeout(timer);
      try { resolve(JSON.parse(line)); } catch (error) { child.kill(); reject(error); }
    });
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`server exited ${code}: ${stderr}`)); });
  });
  return {
    ...ready, child,
    status: async () => {
      const response = await fetch(`${ready.url}/status`, { signal: AbortSignal.timeout(3000) });
      if (!response.ok) throw new Error(`status: ${response.status}`);
      return response.json();
    },
    stop: async () => {
      if (child.exitCode !== null || child.signalCode !== null) throw new Error(`server stopped unexpectedly: ${stderr}`);
      const began = performance.now();
      const exited = new Promise((resolve, reject) => {
        const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error(`server shutdown timed out: ${stderr}`)); }, 2000);
        child.once('exit', (code, signal) => {
          clearTimeout(timer);
          lines.close();
          if (code !== 0 || signal) reject(new Error(`server shutdown ${code}/${signal}: ${stderr}`));
          else resolve(performance.now() - began);
        });
      });
      child.kill('SIGTERM');
      return exited;
    },
  };
}

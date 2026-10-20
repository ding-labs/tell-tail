export function distribution(values) {
  if (!values.length) return null;
  if (values.some(n => !Number.isFinite(n))) throw new Error('non-finite measurement');
  const sorted = values.toSorted((a, b) => a - b);
  const quantile = p => sorted[Math.ceil(sorted.length * p) - 1];
  return { n: sorted.length, min: sorted[0], p50: quantile(.5), p95: quantile(.95), p99: quantile(.99), max: sorted.at(-1) };
}

export function analyze(client, afterCalibration) {
  if (client.overflow || client.errors || client.visibility !== 'visible' || !client.samples.length) {
    throw new Error(`invalid browser run: ${JSON.stringify({ overflow: client.overflow, errors: client.errors, visibility: client.visibility, samples: client.samples.length })}`);
  }
  const offset = client.calibration.offset_ms;
  const age = (sample, at) => at - sample.published_us / 1000 - offset;
  const displayed = client.samples.filter(sample => sample.dom_ms !== undefined);
  return {
    received: client.received, rendered: client.rendered, missed: client.missed,
    clock_uncertainty_ms: client.calibration.rtt_ms / 2,
    clock_offset_change_ms: afterCalibration.offset_ms - offset,
    receipt_ms: distribution(client.samples.map(sample => age(sample, sample.received_ms))),
    decode_ms: distribution(client.samples.map(sample => sample.decode_ms)),
    dom_ms: distribution(displayed.map(sample => age(sample, sample.dom_ms))),
    dom_work_ms: distribution(displayed.map(sample => sample.dom_work_ms)),
    heap_bytes: client.heap_bytes,
  };
}

export function parseProcesses(stdout) {
  return stdout.trim().split('\n').filter(Boolean).map(line => {
    const [pid, ppid, rss, cpu] = line.trim().split(/\s+/);
    return { pid: Number(pid), ppid: Number(ppid), rss_kib: Number(rss), cpu_seconds: cpu.split(':').reduce((a, b) => a * 60 + Number(b), 0) };
  });
}

export function processTree(processes, root) {
  const ids = new Set([root]);
  for (let previous = -1; previous !== ids.size;) {
    previous = ids.size;
    for (const p of processes) if (ids.has(p.ppid)) ids.add(p.pid);
  }
  return processes.filter(p => ids.has(p.pid));
}

export function resources(before, after, observations) {
  const baseline = new Map(before.map(p => [p.pid, p.cpu_seconds]));
  return {
    cpu_seconds: after.reduce((sum, p) => sum + p.cpu_seconds - (baseline.get(p.pid) ?? 0), 0),
    rss_peak_kib: Math.max(...observations.map(ps => ps.reduce((sum, p) => sum + p.rss_kib, 0))),
    exited_processes: before.filter(p => !after.some(q => q.pid === p.pid)).length,
  };
}

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { rmSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { platform, release, arch, cpus } from 'node:os';
import { chromium } from '../../browser-tests/helpers.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const output = process.env.CHART_BENCH_OUTPUT || `${root}work/chart-benchmark`;
const seconds = Number(process.env.CHART_BENCH_SECONDS || 30);
const warmup = Number(process.env.CHART_BENCH_WARMUP || 5);
assert.ok(seconds >= 1 && warmup >= 0);
await mkdir(output, { recursive: true });
await mkdir(`${root}work`, { recursive: true });
const lock = `${root}work/.chart-rendering-benchmark-lock`;
try { await mkdir(lock); }
catch (error) {
  if (error.code !== 'EEXIST') throw error;
  throw new Error(`Another chart benchmark owns ${lock}. Check its pid file before removing a stale lock.`);
}
process.once('exit',()=>rmSync(lock,{recursive:true,force:true}));
await writeFile(`${lock}/pid`,String(process.pid));
const original = await readFile(`${root}web/render.mjs`, 'utf8');
const baselineRef = process.env.CHART_BENCH_BASE_REF || 'cca36ad82271f7827953d5519514c1825dd67255';
const asset = async path => baselineRef
  ? execFileSync('git',['show',`${baselineRef}:web/${path}`],{cwd:root})
  : readFile(`${root}web/${path}`);
const baseline = (await asset('render.mjs')).toString();
const currentHtml = await readFile(`${root}web/index.html`, 'utf8');
const currentCss = await readFile(`${root}web/dashboard.css`);
const adapter = await readFile(new URL('./chartjs.mjs', import.meta.url), 'utf8');
const driver = await readFile(new URL('./driver.mjs', import.meta.url), 'utf8');
const chartjs = await readFile(new URL('./node_modules/chart.js/dist/chart.umd.min.js', import.meta.url));
const html = (await asset('index.html')).toString();
const fixture = await readFile(`${root}tests/fixtures/output/empty-web-v1.json`);
function variantSource(variant) {
  if (variant === 'svg-motion') return original;
  if (variant === 'svg20') return baseline;
  if (variant === 'svg60') return baseline.replace('const chartFrameMs = 1000 / 20;', 'const chartFrameMs = 0;');
  assert.ok(['chartjs', 'chartjs20', 'chartjs30'].includes(variant));
  let source = baseline;
  const drawStart = source.indexOf('function drawChart(vertices) {');
  const drawEnd = source.indexOf('\nexport function stopChartAnimation()', drawStart);
  assert.ok(drawStart > 0 && drawEnd > drawStart);
  source = source.slice(0, drawStart) + 'function drawChart(vertices) { chartVertices = vertices; updateCanvas(vertices, false); }\n' + source.slice(drawEnd);
  const start = source.indexOf('  if (animate) {');
  const end = source.indexOf("  byId('chart-empty')", start);
  assert.ok(start > 0 && end > start);
  source = source.slice(0, start) + '  chartVertices = vertices;\n  updateCanvas(vertices, animate);\n' + source.slice(end);
  return `import { updateCanvas } from '/chartjs.mjs';\n${source}`;
}
const routes = new Map([
  ['/driver.mjs', driver], ['/chartjs.mjs', adapter], ['/chart.umd.min.js', chartjs],
  ['/fixture.json', fixture], ['/frame.mjs', await readFile(`${root}web/frame.mjs`)],
  ['/dashboard.css', await asset('dashboard.css')], ['/favicon.svg', await readFile(`${root}web/favicon.svg`)],
]);
const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  const variant = url.searchParams.get('variant') || req.headers.referer && new URL(req.headers.referer).searchParams.get('variant') || 'svg20';
  let body;
  if (url.pathname === '/') body = (variant==='svg-motion'?currentHtml:html).replace('<script type="module" src="/dashboard.mjs"></script>',
    `${variant.startsWith('chartjs') ? '<script src="/chart.umd.min.js"></script>' : ''}<script type="module" src="/driver.mjs"></script>`);
  else if (url.pathname === '/render.mjs') body = variantSource(variant);
  else if (url.pathname === '/dashboard.css' && variant==='svg-motion') body = currentCss;
  else body = routes.get(url.pathname);
  if (body === undefined) { res.writeHead(404).end(); return; }
  const type = url.pathname.endsWith('.mjs') || url.pathname.endsWith('.js') ? 'text/javascript' : url.pathname.endsWith('.css') ? 'text/css' : url.pathname.endsWith('.json') ? 'application/json' : url.pathname.endsWith('.svg') ? 'image/svg+xml' : 'text/html';
  res.writeHead(200, { 'content-type': type, 'cache-control': 'no-store' }); res.end(body);
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const url = `http://127.0.0.1:${server.address().port}/`;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const cpuSeconds = text => text.split(':').reduce((sum, value) => sum * 60 + Number(value), 0);
function resources(rootPid) {
  const rows = execFileSync('ps', ['-axo', 'pid=,ppid=,rss=,time='], { encoding: 'utf8' }).trim().split('\n').map(line => {
    const [pid, parent, rss, cpu] = line.trim().split(/\s+/);
    return { pid: +pid, parent: +parent, rss: +rss / 1024, cpu: cpuSeconds(cpu) };
  });
  const ids = new Set([rootPid]);
  for (let n = -1; n !== ids.size;) {
    n = ids.size;
    for (const row of rows) if (ids.has(row.parent)) ids.add(row.pid);
  }
  const selected = rows.filter(row => ids.has(row.pid));
  return { cpu: selected.reduce((sum,row) => sum+row.cpu,0), rss: selected.reduce((sum,row) => sum+row.rss,0), pids: selected.map(row=>row.pid).sort((a,b)=>a-b) };
}
const percentile = (values, p) => { const sorted = [...values].sort((a,b)=>a-b); return sorted[Math.min(sorted.length-1, Math.floor(sorted.length*p))]; };
const report = process.env.CHART_BENCH_RESUME === '1'
  ? JSON.parse(await readFile(`${output}/results.json`, 'utf8'))
  : { started: new Date().toISOString(), seconds, warmup, platform: process.platform,
  source: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding:'utf8' }).trim(),
  host: { platform:platform(), release:release(), arch:arch(), cpu:cpus()[0]?.model },
  baselineRef,
  inputs: Object.fromEntries([['render',original],['baseline',baseline],['html',currentHtml],['css',currentCss],['adapter',adapter],['driver',driver],['chartjs',chartjs]]
    .map(([key,value])=>[key,createHash('sha256').update(value).digest('hex')])),
  chartjsVersion: JSON.parse(await readFile(new URL('./node_modules/chart.js/package.json', import.meta.url))).version,
  trials: [] };
assert.equal(report.seconds, seconds);
assert.equal(report.warmup, warmup);
async function trial(variant, index) {
  const launcher = await chromium.launchServer({ channel: process.env.CHART_BENCH_CHANNEL || 'chrome', headless: false });
  const browser = await chromium.connect(launcher.wsEndpoint());
  try {
    report.browserVersion = browser.version();
    const page = await browser.newPage({ viewport: { width:1280, height:1100 }, deviceScaleFactor:1 });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`${url}?variant=${variant}`);
    await page.waitForFunction(() => window.benchReady);
    await sleep(warmup*1000);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Performance.enable');
    await cdp.send('HeapProfiler.collectGarbage');
    const heapBefore = await cdp.send('Runtime.getHeapUsage');
    const metricsBefore = await cdp.send('Performance.getMetrics');
    const first = resources(launcher.process().pid);
    if (process.env.CHART_BENCH_PROFILE==='1') { await cdp.send('Profiler.enable'); await cdp.send('Profiler.start'); }
    const paintRecordingStart = await page.evaluate(() => { window.benchStartRecording(); return performance.now(); });
    const start = performance.now();
    const samples = [];
    let previousWall = Date.now();
    let nextProgress = 60;
    let maxObservationGap = 0;
    while (performance.now()-start < seconds*1000) {
      await sleep(1000);
      const sample = resources(launcher.process().pid);
      sample.seconds = (performance.now()-start)/1000;
      const wall = Date.now();
      maxObservationGap = Math.max(maxObservationGap,(wall-previousWall)/1000);
      assert.ok(wall-previousWall < 5000, 'benchmark observation gap exceeded five seconds');
      previousWall = wall;
      assert.deepEqual(sample.pids, first.pids, 'CPU accounting requires a stable browser process tree');
      assert.equal(await page.evaluate(()=>document.hidden),false,'benchmark page became hidden');
      samples.push(sample);
      if (sample.seconds >= nextProgress) {
        console.log(JSON.stringify({progress:true,variant,index,seconds:sample.seconds,
          cpu_percent:100*(sample.cpu-first.cpu)/sample.seconds,rss_mib:sample.rss}));
        nextProgress += 60;
      }
    }
    const elapsed = (performance.now()-start)/1000;
    const {paints, paintRecordingEnd} = await page.evaluate(() => ({paints:window.benchStopRecording(),paintRecordingEnd:performance.now()}));
    const last = samples.at(-1);
    assert.deepEqual(last.pids, first.pids, 'CPU accounting requires a stable browser process tree');
    assert.deepEqual(errors, []);
    const metricsAfter = await cdp.send('Performance.getMetrics');
    if (process.env.CHART_BENCH_PROFILE==='1') {
      const profile = await cdp.send('Profiler.stop');
      await writeFile(`${output}/${variant}-${index}.cpuprofile`,JSON.stringify(profile.profile));
    }
    const metric = (metrics, name) => metrics.metrics.find(m=>m.name===name).value;
    const gaps = paints.slice(1).map((time,i)=>time-paints[i]);
    assert.ok(gaps.length > 0 && (variant==='svg-motion' || Math.max(...gaps) < 1000), 'benchmark animation stalled for a second');
    assert.ok(variant==='svg-motion' || paints[0]-paintRecordingStart < 1000 && paintRecordingEnd-paints.at(-1) < 1000,
      'benchmark animation stalled at the recording boundary');
    await cdp.send('HeapProfiler.collectGarbage');
    const heap = await cdp.send('Runtime.getHeapUsage');
    const dom = await cdp.send('Memory.getDOMCounters');
    // Probe visible motion separately, so per-frame geometry reads cannot inflate
    // the CPU measurement or prevent compositor-only animation.
    const motion = variant.startsWith('svg') ? await page.evaluate(async () => {
      const dots=[...document.querySelectorAll('#chart-dots circle')];
      const second=dots[Math.floor(dots.length/2)].dataset.second;
      const times=[]; let lastX; const start=performance.now();
      await new Promise(resolve=>{
        const tick=now=>{
          const dot=document.querySelector(`#chart-dots circle[data-second="${second}"]`);
          const x=dot?.getBoundingClientRect().x;
          if (x!==undefined && x!==lastX) {times.push(now);lastX=x;}
          if(now-start<4000)requestAnimationFrame(tick);else resolve();
        };requestAnimationFrame(tick);
      });
      return {times,seconds:(performance.now()-start)/1000};
    }) : null;
    const motionGaps=motion ? motion.times.slice(1).map((time,i)=>time-motion.times[i]) : [];
    assert.ok(!motion || motionGaps.length>0 && Math.max(...motionGaps)<1000,'visible chart motion stalled');
    // Capture a stable reference and then verify sustained intermediate motion.
    await page.evaluate(() => { window.benchPause(); window.benchFrame(120); });
    await page.screenshot({ path:`${output}/${variant}-${index}.png` });
    const coordinates = () => page.evaluate(() => window.benchCoordinates?.() ?? [...document.querySelectorAll('#chart-dots circle')].map(p=>{
      if (!document.getElementById('chart-motion')) return {x:+p.getAttribute('cx'),y:+p.getAttribute('cy')};
      const box=p.getBoundingClientRect(),svg=document.getElementById('chart').getBoundingClientRect();
      return {x:(box.x+box.width/2-svg.x)*760/svg.width,y:(box.y+box.height/2-svg.y)*224/svg.height};
    }));
    const stable = await coordinates();
    const rect = await page.locator('#chart').boundingBox();
    const scale = variant.startsWith('chartjs') ? rect.width/760 : 1;
    assert.ok(Math.abs(stable[0].x/scale-60)<1, 'first point aligns with the SVG plot origin');
    assert.ok(Math.abs(stable.at(-1).x/scale-748)<1, 'last point aligns with the SVG plot edge');
    await page.evaluate(() => window.benchFrame(121));
    const positions = [await coordinates()];
    for (let i=0;i<3;i++) { await sleep(250); positions.push(await coordinates()); }
    assert.ok(positions.every(p=>p.length===60));
    for(let i=1;i<positions.length;i++) assert.notEqual(positions[i][20].x,positions[i-1][20].x,'a retained second must scroll smoothly throughout the transition');
    assert.ok(positions.every(p=>p.every(v=>Number.isFinite(v.x)&&Number.isFinite(v.y))));
    await page.evaluate(() => window.benchPause());
    const stopped = await coordinates(); await sleep(250); assert.deepEqual(await coordinates(),stopped);
    const trial = {variant,index,elapsed,cpu_percent:100*(last.cpu-first.cpu)/elapsed,
      rss_mib_median:percentile(samples.map(s=>s.rss),.5),rss_mib_max:Math.max(...samples.map(s=>s.rss)),
      js_heap_bytes:heap.usedSize,dom,paint_hz:paints.length/elapsed,
      visible_motion_hz:motion ? motion.times.length/motion.seconds : undefined,
      visible_motion_gap_ms_p95:motion ? percentile(motionGaps,.95) : undefined,
      js_heap_before_bytes:heapBefore.usedSize,max_observation_gap_seconds:maxObservationGap,
      paint_gap_ms_max:Math.max(...gaps),
      first_paint_delay_ms:paints[0]-paintRecordingStart,last_paint_age_ms:paintRecordingEnd-paints.at(-1),
      paint_gap_ms_median:percentile(gaps,.5),paint_gap_ms_p95:percentile(gaps,.95),
      renderer_task_cpu_percent:100*(metric(metricsAfter,'TaskDuration')-metric(metricsBefore,'TaskDuration'))/elapsed,
      positions, samples};
    report.trials.push(trial);
    await writeFile(`${output}/results.json`,JSON.stringify(report,null,2));
    console.log(JSON.stringify({...trial,positions:undefined,samples:undefined}));
  } finally { await browser.close(); await launcher.close(); }
}
try {
  const variants = (process.env.CHART_BENCH_ORDER || 'svg20,chartjs20,chartjs,svg60,svg60,chartjs,chartjs20,svg20').split(',');
  const offset = report.trials.length;
  for (let index=0;index<variants.length;index++) {
    for (let attempt=0;attempt<3;attempt++) {
      try { await trial(variants[index],offset+index); break; }
      catch (error) {
        if (!error.message.includes('CPU accounting requires a stable browser process tree') || attempt === 2) throw error;
        (report.rejectedTrials ??= []).push({variant:variants[index],reason:'browser process tree changed',attempt});
        await writeFile(`${output}/results.json`,JSON.stringify(report,null,2));
        console.log(`Discarding ${variants[index]} trial: browser process tree changed; retrying with a fresh browser.`);
      }
    }
  }
  report.finished = new Date().toISOString();
  await writeFile(`${output}/results.json`,JSON.stringify(report,null,2));
} finally { await new Promise(resolve=>server.close(resolve)); }

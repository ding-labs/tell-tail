// Real Apple unified-log/browser soak. No raw log records are persisted.
import { chromium, monitor, root } from '../../browser-tests/helpers.mjs';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdir, writeFile, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { cpus, release } from 'node:os';
const seconds = Number(process.env.SOAK_SECONDS || 900);
const warmup = Number(process.env.SOAK_WARMUP || 120);
const interval = Number(process.env.SOAK_BLOCK || 180);
for (const n of [seconds, warmup, interval]) if (!Number.isFinite(n) || n < 1) throw new Error('durations must be positive seconds');
const output = resolve(process.env.SOAK_OUTPUT || `${root}/work/live-browser-soak`);
const binary = resolve(root, process.env.TELL_TAIL_BIN || 'target/release/tell-tail');
const lock = resolve(root, 'work/.chart-rendering-benchmark-lock');
await mkdir(output, { recursive: true });
await mkdir(lock);
await writeFile(`${lock}/pid`, String(process.pid));
const report = { started: new Date().toISOString(), source: execFileSync('git', ['rev-parse', 'HEAD'], {cwd:root, encoding:'utf8'}).trim(), binary_sha256:createHash('sha256').update(readFileSync(binary)).digest('hex'), host:{cpu:cpus()[0].model,release:release()}, seconds,warmup,interval, phases:[], heaps:[], errors:[] };
const save = () => writeFile(`${output}/results.json`, JSON.stringify(report, null, 2));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const cpu = s => s.split(':').reduce((a,v) => a*60+Number(v), 0);
function processes() {
  return execFileSync('ps', ['-axo','pid=,ppid=,rss=,time='], {encoding:'utf8'}).trim().split('\n').map(line => {
    const [pid,parent,rss,time] = line.trim().split(/\s+/);
    return {pid:+pid,parent:+parent,rss:+rss/1024,cpu:cpu(time)};
  });
}
function tree(rows, pid) {
  const ids = new Set([pid]);
  for (let n=-1; n!==ids.size;) { n=ids.size; for (const r of rows) if(ids.has(r.parent)) ids.add(r.pid); }
  return rows.filter(r=>ids.has(r.pid));
}
let server, launcher, browser, page, cdp;
function resources() {
  const rows=processes();
  const groups={monitor:rows.filter(r=>r.pid===server.child.pid),capture:tree(rows,server.child.pid).filter(r=>r.pid!==server.child.pid),browser:tree(rows,launcher.process().pid)};
  return Object.fromEntries(Object.entries(groups).map(([name,rs])=>[name,{cpu:rs.reduce((s,r)=>s+r.cpu,0),rss:rs.reduce((s,r)=>s+r.rss,0),pids:rs.map(r=>r.pid).sort((a,b)=>a-b)}]));
}
async function metrics() { const response=await fetch(`${server.url}api/snapshot`); if(!response.ok)throw new Error(`snapshot ${response.status}`); return (await response.json()).metrics; }
function descriptors() {
  const fd=spawnSync('/usr/sbin/lsof',['-nP','-a','-p',String(server.child.pid),'-Ff'],{encoding:'utf8',timeout:5000});
  return {status:fd.status,count:fd.stdout.split('\n').filter(x=>/^f\d+$/.test(x)).length};
}
async function heap(label) {
  await cdp.send('HeapProfiler.collectGarbage');
  const h={label,at:new Date().toISOString(),elapsed_seconds:(performance.now()-started)/1000,...await cdp.send('Runtime.getHeapUsage'),...await cdp.send('Memory.getDOMCounters'),descriptors:descriptors(),...await page.evaluate(()=>({elements:document.querySelectorAll('*').length,dots:document.querySelectorAll('#chart-dots circle').length,history_rows:document.querySelectorAll('#chart-values tr').length,path_segments:(document.getElementById('chart-line').getAttribute('d').match(/ C/g)||[]).length}))};
  report.heaps.push(h); await save(); console.log(JSON.stringify({heap:h}));
}
async function phase(name, duration, visible=true) {
  const begin=performance.now(), first=resources(), before=await metrics();
  const p={name,started:new Date().toISOString(),first,before,samples:[],process_changes:[]};
  report.phases.push(p);
  while(performance.now()-begin<duration*1000) {
    await sleep(1000);
    const s={seconds:(performance.now()-begin)/1000,...resources()};
    if(visible) {
      const ui=await page.evaluate(()=>({hidden:document.hidden,connection:document.getElementById('connection').textContent,chart:(()=>{const r=document.getElementById('chart').getBoundingClientRect();return r.bottom>0&&r.top<innerHeight&&r.left<innerWidth&&r.right>0;})()}));
      if(ui.hidden||!ui.chart||!ui.connection.startsWith('Live')) throw new Error(`dashboard is not live and visible: ${JSON.stringify(ui)}`);
    }
    const gap=s.seconds-(p.samples.at(-1)?.seconds||0);
    if(gap>5)throw new Error(`resource observation gap ${gap}`);
    for(const g of ['monitor','capture','browser']) {
      if(!s[g].pids.length)throw new Error(`${g} exited`);
      if(JSON.stringify(s[g].pids)!==JSON.stringify((p.samples.at(-1)||first)[g].pids))p.process_changes.push({seconds:s.seconds,group:g,pids:s[g].pids});
    }
    p.samples.push(s);
    if(p.samples.length%30===0) {await save();console.log(JSON.stringify({progress:name,seconds:s.seconds,resources:s}));}
  }
  const last=p.samples.at(-1); p.seconds=last.seconds; p.after=await metrics(); p.groups={};
  p.records=Number(p.after.totals.records)-Number(before.totals.records);
  p.bytes=Number(p.after.bytes_read)-Number(before.bytes_read);
  for(const g of ['monitor','capture','browser']) {
    const rss=p.samples.map(s=>s[g].rss).sort((a,b)=>a-b);
    p.groups[g]={cpu_percent:p.process_changes.some(c=>c.group===g)?null:100*(last[g].cpu-first[g].cpu)/p.seconds,rss_mib_median:rss[Math.floor(rss.length/2)],rss_mib_min:rss[0],rss_mib_max:rss.at(-1),rss_mib_first:p.samples[0][g].rss,rss_mib_last:last[g].rss};
  }
  await save();console.log(JSON.stringify({phase:name,seconds:p.seconds,records:p.records,groups:p.groups,process_changes:p.process_changes}));
}
const started=performance.now();
try {
  server=await monitor(['--apple-unified']);
  launcher=await chromium.launchServer({channel:'chrome',headless:false});
  browser=await chromium.connect(launcher.wsEndpoint());report.browserVersion=browser.version();
  report.pids={monitor:server.child.pid,browser:launcher.process().pid};
  page=await browser.newPage({viewport:{width:1280,height:1100},deviceScaleFactor:1});
  page.on('pageerror',e=>report.errors.push(String(e)));
  await page.goto(server.url);
  await page.waitForFunction(()=>document.getElementById('connection').textContent.startsWith('Live'));
  await page.evaluate(()=>document.getElementById('chart').scrollIntoView({block:'center'}));
  await page.screenshot({path:`${output}/dashboard.png`});
  cdp=await page.context().newCDPSession(page);
  await phase('warmup',warmup);
  await heap('after warmup');
  for(let elapsed=0,block=1;elapsed<seconds;block++) {
    const duration=Math.min(interval,seconds-elapsed);
    await phase(`live ${block}`,duration);elapsed+=duration;await heap(`live ${elapsed}s`);
  }
  // Evaluate the button directly, avoiding locator click instrumentation in heap comparisons.
  await page.evaluate(()=>document.getElementById('freeze').click());
  await phase('frozen, capture continues',Number(process.env.SOAK_FROZEN || 30));await heap('frozen');
  await page.evaluate(()=>document.getElementById('freeze').click());
  await phase('resumed',Number(process.env.SOAK_RESUMED || 60));await heap('resumed');
  const leaks=spawnSync('/usr/bin/leaks',[String(server.child.pid)],{encoding:'utf8',timeout:30000});
  report.leaks={status:leaks.status,stdout:leaks.stdout,stderr:leaks.stderr};
  report.finalMetrics=await metrics();
} catch(error) {report.failure=String(error.stack||error);throw error;}
finally {
  if(browser)await browser.close();
  if(launcher)await launcher.close();
  if(server)await server.close();
  report.finished=new Date().toISOString();
  const owned=new Set(Object.values(report.pids||{}));
  for(const p of report.phases) for(const s of p.samples) for(const g of ['monitor','capture','browser']) for(const pid of s[g].pids) owned.add(pid);
  report.remaining_owned_pids=[...owned].filter(pid=>{try{process.kill(pid,0);return true;}catch{return false;}});
  await save();await rm(lock,{recursive:true});
}

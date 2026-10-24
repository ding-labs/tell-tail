// Diagnose native Chrome memory separately from retained dashboard objects.
import {chromium, monitor, root} from '../../browser-tests/helpers.mjs';
import {execFileSync} from 'node:child_process';
import {mkdir,writeFile,rm} from 'node:fs/promises';
import {createWriteStream,readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {once} from 'node:events';
import {resolve} from 'node:path';
const output=resolve(process.env.MEMORY_OUTPUT||`${root}/work/chrome-memory`);
const warmup=Number(process.env.MEMORY_WARMUP||120);
const block=Number(process.env.MEMORY_BLOCK||300);
const blocks=Number(process.env.MEMORY_BLOCKS||3);
const recovery=Number(process.env.MEMORY_RECOVERY||300);
const control=Number(process.env.MEMORY_CONTROL||60);
const profile=Number(process.env.MEMORY_PROFILE||120);
for(const n of [warmup,block,blocks,recovery,control,profile])if(!Number.isFinite(n)||n<1)throw new Error('durations/count must be positive');
const lock=resolve(root,'work/.chart-rendering-benchmark-lock');
await mkdir(output,{recursive:true});await mkdir(lock);await writeFile(`${lock}/pid`,String(process.pid));
const report={started:new Date().toISOString(),source:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),binary_sha256:createHash('sha256').update(readFileSync(resolve(root,process.env.TELL_TAIL_BIN||'target/release/tell-tail'))).digest('hex'),config:{warmup,block,blocks,recovery,control,profile},phases:[],checks:[],errors:[],owned_pids:[]};
const save=()=>writeFile(`${output}/results.json`,JSON.stringify(report,null,2));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let server,launcher,browser,page,cdp,browserCDP;
function resources(){
 const rows=execFileSync('ps',['-axo','pid=,ppid=,rss=,args='],{encoding:'utf8'}).trim().split('\n').map(line=>{
  const [,pid,parent,rss,args]=line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
  const role=args.match(/--utility-sub-type=([^\s]+)/)?.[1]||args.match(/--type=([^\s]+)/)?.[1]||'browser';
  return {pid:+pid,parent:+parent,rss_mib:+rss/1024,role};
 });
 const ids=new Set([launcher.process().pid]);
 for(let n=-1;n!==ids.size;){n=ids.size;for(const r of rows)if(ids.has(r.parent))ids.add(r.pid);}
 const rs=rows.filter(r=>ids.has(r.pid));
 for(const r of rs)if(!report.owned_pids.includes(r.pid))report.owned_pids.push(r.pid);
 const capture=new Set([server.child.pid]);
 for(let n=-1;n!==capture.size;){n=capture.size;for(const r of rows)if(capture.has(r.parent))capture.add(r.pid);}
 for(const pid of capture)if(!report.owned_pids.includes(pid))report.owned_pids.push(pid);
 return {at:new Date().toISOString(),total_mib:rs.reduce((s,r)=>s+r.rss_mib,0),processes:rs};
}
async function ui(){
 return page.evaluate(()=>{
  const el=document.getElementById('chart');const r=el?.getBoundingClientRect();
  return {hidden:document.hidden,visible:!!r&&r.bottom>0&&r.top<innerHeight,connection:document.getElementById('connection')?.textContent,elements:document.querySelectorAll('*').length,dots:document.querySelectorAll('#chart-dots circle').length,rows:document.querySelectorAll('#chart-values tr').length,animations:document.getAnimations().length};
 });
}
async function phase(name,seconds,live=true){
 const p={name,started:new Date().toISOString(),samples:[],before:resources()};report.phases.push(p);
 const start=performance.now();
 while(performance.now()-start<seconds*1000){
  await sleep(Math.min(5000,seconds*1000));const s=resources();s.seconds=(performance.now()-start)/1000;
  const gap=s.seconds-(p.samples.at(-1)?.seconds||0);if(gap>10)throw new Error(`sampling gap ${gap}`);
  p.samples.push(s);
  if(p.samples.length%6===0){await save();console.log(JSON.stringify({progress:name,seconds:s.seconds,rss_mib:s.total_mib}));}
 }
 p.seconds=(performance.now()-start)/1000;
 if(live){p.ui=await ui();if(p.ui.hidden||!p.ui.visible||!p.ui.connection?.startsWith('Live'))throw new Error(`not live/visible: ${JSON.stringify(p.ui)}`);}
 await save();console.log(JSON.stringify({phase:name,seconds:p.seconds,rss_mib:p.samples.at(-1).total_mib}));
}
async function heapSnapshot(label){
 const path=`${output}/${label}.heapsnapshot`;const stream=createWriteStream(path);
 const onChunk=({chunk})=>stream.write(chunk);
 cdp.on('HeapProfiler.addHeapSnapshotChunk',onChunk);
 try{await cdp.send('HeapProfiler.takeHeapSnapshot',{reportProgress:false,captureNumericValue:true});}
 finally{cdp.off('HeapProfiler.addHeapSnapshotChunk',onChunk);stream.end();await once(stream,'finish');}
 return path;
}
async function nativeDump(label){
 const events=[];const onData=({value})=>events.push(...value);
 browserCDP.on('Tracing.dataCollected',onData);
 let dump;
 try{
  await browserCDP.send('Tracing.start',{traceConfig:{includedCategories:['disabled-by-default-memory-infra'],excludedCategories:['*'],memoryDumpConfig:{},traceBufferSizeInKb:8192}});
  dump=await browserCDP.send('Tracing.requestMemoryDump',{deterministic:true,levelOfDetail:'detailed'});
  const done=new Promise(resolve=>browserCDP.once('Tracing.tracingComplete',resolve));
  await browserCDP.send('Tracing.end');await done;
 }finally{browserCDP.off('Tracing.dataCollected',onData);}
 await writeFile(`${output}/${label}.memory.json`,JSON.stringify({dump,traceEvents:events}));
 return {success:dump.success,guid:dump.dumpGuid,events:events.length};
}
async function check(label,{snapshot=false,native=false,live=true}={}){
 const c={label,at:new Date().toISOString(),before:resources()};report.checks.push(c);
 if(cdp){await cdp.send('HeapProfiler.collectGarbage');c.heap=await cdp.send('Runtime.getHeapUsage');c.dom=await cdp.send('Memory.getDOMCounters');c.ui=await ui();}
 if(snapshot&&cdp)c.snapshot=await heapSnapshot(label);
 if(native)c.dump=await nativeDump(label);c.after=resources();
 if(live&&c.ui&&(c.ui.hidden||!c.ui.visible||!c.ui.connection?.startsWith('Live')))throw new Error('dashboard lost visibility/connection');
 await save();console.log(JSON.stringify({check:label,heap:c.heap,dom:c.dom,rss_before:c.before.total_mib,rss_after:c.after.total_mib,dump:c.dump}));
}
async function openDashboard(){
 page=await browser.newPage({viewport:{width:1280,height:1100},deviceScaleFactor:1});page.on('pageerror',e=>report.errors.push(String(e)));
 await page.goto(server.url);await page.waitForFunction(()=>document.getElementById('connection').textContent.startsWith('Live'));
 await page.evaluate(()=>document.getElementById('chart').scrollIntoView({block:'center'}));
 cdp=await page.context().newCDPSession(page);
}
try{
 server=await monitor(['--apple-unified']);report.monitor_pid=server.child.pid;
 launcher=await chromium.launchServer({channel:'chrome',headless:false});browser=await chromium.connect(launcher.wsEndpoint());report.browser=browser.version();
 browserCDP=await browser.newBrowserCDPSession();await openDashboard();
 await phase('warmup',warmup);await check('baseline');
 await phase('baseline-settle',control);await check('settled');
 for(let i=1;i<=blocks;i++){await phase(`live-${i}`,block);await check(`live-${i}`);}
 await browserCDP.send('Memory.simulatePressureNotification',{level:'critical'});
 await phase('pressure-live',control);await check('pressure-live');
 await phase('recovery-live',recovery);await check('recovery-live');
 await page.evaluate(()=>document.getElementById('freeze').click());await phase('frozen',control);await check('frozen');
 await page.evaluate(()=>document.getElementById('freeze').click());await phase('resumed',control);await check('resumed');
 // Heavy diagnostics begin only after the natural-growth and pressure controls.
 await check('profile-before',{snapshot:true,native:true});
 await phase('profiled-live',profile);await check('profile-after',{snapshot:true,native:true});
 await page.goto('about:blank');await phase('blank',control,false);await check('blank',{snapshot:true,native:true,live:false});
 await page.close();page=null;cdp=null;await phase('closed-page',control,false);await check('closed-page',{live:false});
 await browserCDP.send('Memory.simulatePressureNotification',{level:'critical'});
 await phase('closed-pressure',control,false);await check('closed-pressure',{native:true,live:false});
 await openDashboard();await phase('reopened',control);await check('reopened',{snapshot:true,native:true});
 report.finalMetrics=(await(await fetch(`${server.url}api/snapshot`)).json()).metrics;
}catch(e){report.failure=String(e.stack||e);throw e;}
finally{
 if(browser)await browser.close();if(launcher)await launcher.close();if(server)await server.close();
 report.remaining_owned_pids=[...report.owned_pids,report.monitor_pid].filter(pid=>{try{process.kill(pid,0);return true;}catch{return false;}});
 report.finished=new Date().toISOString();await save();await rm(lock,{recursive:true});
}

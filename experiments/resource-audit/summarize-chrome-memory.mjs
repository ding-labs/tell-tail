import {readFile} from 'node:fs/promises';
const r=JSON.parse(await readFile(process.argv[2],'utf8'));
const median=values=>{const a=[...values].sort((x,y)=>x-y);return a.length?a[Math.floor(a.length/2)]:null;};
const roles=sample=>{const totals={};for(const p of sample.processes)totals[p.role]=(totals[p.role]||0)+p.rss_mib;return totals;};
const phases=r.phases.map(p=>{
 const roleNames=new Set(p.samples.flatMap(s=>s.processes.map(p=>p.role)));
 return {name:p.name,seconds:p.seconds,median_total_mib:median(p.samples.map(s=>s.total_mib)),last30_median_mib:median(p.samples.filter(s=>s.seconds>p.seconds-30).map(s=>s.total_mib)),roles:Object.fromEntries([...roleNames].map(role=>[role,median(p.samples.map(s=>roles(s)[role]||0))])),pids:[...new Set(p.samples.flatMap(s=>s.processes.map(p=>p.pid)))]};
});
const checks=r.checks.map(c=>({label:c.label,rss_before_mib:c.before.total_mib,rss_after_mib:c.after.total_mib,roles_before:roles(c.before),roles_after:roles(c.after),heap_mib:c.heap?.usedSize/1048576,embedder_mib:c.heap?.embedderHeapUsedSize/1048576,...c.dom,ui:c.ui,heavy_profiling:!!(c.snapshot||c.dump)}));
const live=r.phases.filter(p=>/^live-\d+$/.test(p.name));
const byPid=new Map();
for(const p of live)for(const s of p.samples)for(const row of s.processes){
 const entry=byPid.get(row.pid)||{pid:row.pid,role:row.role,values:[],first:row.rss_mib};entry.values.push(row.rss_mib);entry.last=row.rss_mib;byPid.set(row.pid,entry);
}
const processes=[...byPid.values()].map(p=>({pid:p.pid,role:p.role,median_mib:median(p.values),min_mib:Math.min(...p.values),max_mib:Math.max(...p.values),first_mib:p.first,last_mib:p.last}));
console.log(JSON.stringify({source:r.source,binary:r.binary_sha256,browser:r.browser,started:r.started,finished:r.finished,failure:r.failure,errors:r.errors,phases,checks,live_processes:processes,finalMetrics:r.finalMetrics,remaining_owned_pids:r.remaining_owned_pids},null,2));

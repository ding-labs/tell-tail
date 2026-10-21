import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const dir=resolve(process.argv[2]);
const r=JSON.parse(await readFile(`${dir}/results.json`,'utf8'));
const observations=JSON.parse(await readFile(`${dir}/footprint-observations.json`,'utf8'));
const median=a=>{const b=[...a].sort((x,y)=>x-y);return b[Math.floor(b.length/2)];};
const stats=bytes=>({samples:bytes.length,median_mib:median(bytes)/1048576,min_mib:Math.min(...bytes)/1048576,max_mib:Math.max(...bytes)/1048576,first_mib:bytes[0]/1048576,last_mib:bytes.at(-1)/1048576});
const roles=new Map(r.checks.flatMap(c=>[...c.before.processes,...c.after.processes]).map(p=>[p.pid,p.role]));
const heavyAt=Date.parse(r.checks.find(c=>c.label==='profile-before')?.at||'9999-01-01');
const groups=new Map();
for(const o of observations){
 // The observer reads a periodically flushed report. Assign phases by the final
 // timestamps, not by its possibly stale phase label.
 const at=Date.parse(o.at);const p=r.phases.findLast(p=>Date.parse(p.started)<=at);
 const label=at>=heavyAt?`profiled:${p?.name||'unknown'}`:p?.name||'unknown';
 if(!groups.has(label))groups.set(label,[]);groups.get(label).push(o);
}
const summaries=[...groups].map(([phase,rows])=>({phase,total:stats(rows.map(o=>o.total_bytes)),processes:[...new Set(rows.flatMap(o=>o.processes.map(p=>p.pid)))].map(pid=>({pid,role:roles.get(pid),...stats(rows.flatMap(o=>o.processes.filter(p=>p.pid===pid).map(p=>p.bytes)))}))}));
console.log(JSON.stringify({started:observations[0]?.at,ended:observations.at(-1)?.at,samples:observations.length,errors:observations.flatMap(o=>o.errors),warnings:observations.flatMap(o=>o.warnings),summaries},null,2));

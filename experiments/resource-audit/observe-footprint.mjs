// Kernel-accounted footprints for only the Chrome tree owned by a saved run.
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {execFileSync,spawnSync} from 'node:child_process';
import {resolve} from 'node:path';
const dir=resolve(process.argv[2]);const once=process.env.FOOTPRINT_ONCE==='1';
await mkdir(`${dir}/footprints`,{recursive:true});const observations=[];
while(true){
 const r=JSON.parse(await readFile(`${dir}/results.json`,'utf8'));
 if(r.finished)break;
 const first=r.checks[0]?.before.processes.find(p=>p.role==='browser');if(!first)throw new Error('browser root not recorded yet');
 const rows=execFileSync('ps',['-axo','pid=,ppid='],{encoding:'utf8'}).trim().split('\n').map(s=>s.trim().split(/\s+/).map(Number));
 const ids=new Set([first.pid]);for(let n=-1;n!==ids.size;){n=ids.size;for(const [pid,parent]of rows)if(ids.has(parent))ids.add(pid);}
 const active=[...ids].filter(pid=>rows.some(row=>row[0]===pid));if(!active.length)break;
 const at=new Date().toISOString(),file=`${dir}/footprints/${at.replace(/[:.]/g,'-')}.json`;
 const args=active.flatMap(pid=>['-p',String(pid)]).concat(['--noCategories','-f','bytes','-j',file]);
 const command=spawnSync('/usr/bin/footprint',args,{encoding:'utf8',timeout:10000});
 if(command.status!==0)throw new Error(command.stderr||`footprint exit ${command.status}`);
 const data=JSON.parse(await readFile(file,'utf8'));
 observations.push({at,phase:r.phases.at(-1)?.name,processes:data.processes.map(p=>({pid:p.pid,name:p.name,bytes:p.footprint,peak_bytes:p.auxiliary.phys_footprint_peak})),total_bytes:data['total footprint'],errors:data.errors,warnings:data.warnings});
 await writeFile(`${dir}/footprint-observations.json`,JSON.stringify(observations,null,2));
 console.log(JSON.stringify(observations.at(-1)));
 if(once)break;await new Promise(resolve=>setTimeout(resolve,30000));
}

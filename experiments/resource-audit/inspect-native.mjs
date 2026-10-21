// Chrome memory-infra byte scalars are hexadecimal strings. Parent/child
// allocators overlap; report paths individually rather than summing the tree.
import {readFile} from 'node:fs/promises';
export async function inspectNative(path){
 const trace=JSON.parse(await readFile(path,'utf8'));const processes=new Map();
 for(const event of trace.traceEvents){
  const d=event.args?.dumps;if(!d)continue;
  const p=processes.get(event.pid)||{pid:event.pid,allocators:{}};
  if(d.process_totals?.private_footprint_bytes)p.private_footprint_bytes=parseInt(d.process_totals.private_footprint_bytes,16);
  for(const [name,allocator] of Object.entries(d.allocators||{})){
   const attrs={};for(const [key,a]of Object.entries(allocator.attrs||{}))if(a.type==='scalar')attrs[key]=parseInt(a.value,16);
   if(Object.keys(attrs).length)p.allocators[name]=attrs;
  }
  processes.set(event.pid,p);
 }
 return {path,success:trace.dump.success,processes:[...processes.values()]};
}
if(process.argv[1]?.endsWith('/inspect-native.mjs')){
 const reports=await Promise.all(process.argv.slice(2).map(inspectNative));
 console.log(JSON.stringify(reports.map(r=>({...r,processes:r.processes.map(p=>({...p,allocators:Object.fromEntries(Object.entries(p.allocators).filter(([name])=>!name.includes('/')||/^(gpu\/angle\/metal|gpu\/skia|partition_alloc\/partitions|v8\/main\/heap|malloc\/allocated_objects|blink_gc\/main\/allocated_objects|skia\/sk_resource_cache)$/.test(name)))}))})),null,2));
}

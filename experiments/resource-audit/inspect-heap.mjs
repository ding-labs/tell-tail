// Census/diff V8 snapshots without executing their contents.
import {readFile} from 'node:fs/promises';
export async function inspect(path){
 const s=JSON.parse(await readFile(path,'utf8'));
 const f=s.snapshot.meta.node_fields,w=f.length,types=s.snapshot.meta.node_types[f.indexOf('type')];
 const offsets=Object.fromEntries(['type','name','id','self_size','detachedness'].map(k=>[k,f.indexOf(k)]));
 for(const k of ['type','name','id','self_size'])if(offsets[k]<0)throw new Error(`missing ${k}`);
 const groups=new Map(),ids=new Map(),detached=[];let bytes=0;
 for(let i=0;i<s.nodes.length;i+=w){
  const type=types[s.nodes[i+offsets.type]],name=s.strings[s.nodes[i+offsets.name]],size=s.nodes[i+offsets.self_size],id=s.nodes[i+offsets.id];
  const key=`${type}:${name}`;const g=groups.get(key)||{type,name,count:0,bytes:0};g.count++;g.bytes+=size;groups.set(key,g);bytes+=size;ids.set(id,{key,size});
  // DevTools DOMLinkState: low two bits are unknown=0, attached=1, detached=2.
  if(offsets.detachedness>=0&&(s.nodes[i+offsets.detachedness]&3)===2)detached.push({id,type,name,size});
 }
 return {path,nodes:s.nodes.length/w,bytes,groups,ids,detached};
}
export function compare(a,b){
 const deltas=[];for(const key of new Set([...a.groups.keys(),...b.groups.keys()])){
  const x=a.groups.get(key),y=b.groups.get(key);const count=(y?.count||0)-(x?.count||0),bytes=(y?.bytes||0)-(x?.bytes||0);
  if(count||bytes)deltas.push({key:key.slice(0,180),before:x?.count||0,after:y?.count||0,count,bytes});
 }
 let common=0;for(const id of b.ids.keys())if(a.ids.has(id))common++;
 return {before:a.path,after:b.path,node_delta:b.nodes-a.nodes,byte_delta:b.bytes-a.bytes,common_ids:common,new_ids:b.ids.size-common,removed_ids:a.ids.size-common,largest_byte_increases:deltas.filter(d=>d.bytes>0).sort((x,y)=>y.bytes-x.bytes).slice(0,25),largest_byte_decreases:deltas.filter(d=>d.bytes<0).sort((x,y)=>x.bytes-y.bytes).slice(0,15)};
}
if(process.argv[1]?.endsWith('/inspect-heap.mjs')){
 const snapshots=await Promise.all(process.argv.slice(2).map(inspect));
 const summaries=snapshots.map(s=>({path:s.path,nodes:s.nodes,bytes:s.bytes,detached:s.detached,relevant:[...s.groups.values()].filter(g=>['native','object'].includes(g.type)&&/^(Animation|KeyframeEffect|EventSource|SVGCircleElement|SVGTitleElement|HTMLTableRowElement|HTMLTableCellElement|Document|Array|Object)$/.test(g.name)),largest_groups:[...s.groups.values()].sort((a,b)=>b.bytes-a.bytes).slice(0,15).map(g=>({...g,name:g.name.slice(0,160)}))}));
 console.log(JSON.stringify({summaries,diffs:snapshots.slice(1).map((s,i)=>compare(snapshots[i],s))},null,2));
}

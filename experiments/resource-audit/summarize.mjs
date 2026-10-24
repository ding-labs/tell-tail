import {readFile} from 'node:fs/promises';
const report=JSON.parse(await readFile(process.argv[2], 'utf8'));
const live=report.phases.filter(p=>/^live /.test(p.name)&&p.groups);
const median=values=>{const s=[...values].sort((a,b)=>a-b);return s[Math.floor(s.length/2)];};
function slope(points) {
  if(points.length<2)return null;
  const x=points.reduce((s,p)=>s+p[0],0)/points.length,y=points.reduce((s,p)=>s+p[1],0)/points.length;
  return points.reduce((s,p)=>s+(p[0]-x)*(p[1]-y),0)/points.reduce((s,p)=>s+(p[0]-x)**2,0);
}
const heaps=report.heaps.filter(h=>h.label==='after warmup'||/^live /.test(h.label));
const result={source:report.source,browser:report.browserVersion,host:report.host,finished:report.finished,failure:report.failure,errors:report.errors,live_seconds:live.reduce((s,p)=>s+p.seconds,0),live_records:live.reduce((s,p)=>s+p.records,0),live_bytes:live.reduce((s,p)=>s+p.bytes,0),groups:{},heaps:heaps.map(h=>({label:h.label,used_heap_mib:h.usedSize/1048576,embedder_heap_mib:h.embedderHeapUsedSize/1048576,backing_bytes:h.backingStorageSize,documents:h.documents,nodes:h.nodes,listeners:h.jsEventListeners,elements:h.elements,dots:h.dots,rows:h.history_rows,path_segments:h.path_segments,descriptors:h.descriptors.count})),process_changes:report.phases.flatMap(p=>p.process_changes),remaining_owned_pids:report.remaining_owned_pids};
for(const group of ['monitor','capture','browser']) {
  const valid=live.filter(p=>p.groups[group].cpu_percent!==null);
  const samples=live.flatMap(p=>p.samples.map(s=>({time:(Date.parse(p.started)-Date.parse(live[0].started))/60000+s.seconds/60,rss:s[group].rss})));
  const bins=new Map();for(const s of samples){const bin=Math.floor(s.time);if(!bins.has(bin))bins.set(bin,[]);bins.get(bin).push(s);}
  // Exclude short trailing bins so a single observation cannot stand in for a minute.
  const minuteMedians=[...bins].filter(([,values])=>values.at(-1).time-values[0].time>=.75)
    .map(([minute,values])=>[minute,median(values.map(s=>s.rss))]);
  result.groups[group]={cpu_percent:valid.reduce((s,p)=>s+p.groups[group].cpu_percent*p.seconds,0)/valid.reduce((s,p)=>s+p.seconds,0),cpu_valid_seconds:valid.reduce((s,p)=>s+p.seconds,0),rss_mib_median:median(samples.map(s=>s.rss)),rss_mib_range:[Math.min(...samples.map(s=>s.rss)),Math.max(...samples.map(s=>s.rss))],first_minute_median_mib:minuteMedians[0]?.[1],last_minute_median_mib:minuteMedians.at(-1)?.[1],rss_slope_mib_per_minute:slope(minuteMedians),minute_medians:minuteMedians};
}
result.heap_change_bytes=heaps.at(-1)?.usedSize-heaps[0]?.usedSize;
result.heap_slope_kib_per_minute=slope(heaps.map(h=>[h.elapsed_seconds/60,h.usedSize/1024]));
console.log(JSON.stringify(result,null,2));

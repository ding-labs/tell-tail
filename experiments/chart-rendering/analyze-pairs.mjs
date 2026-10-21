import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const path = process.argv[2];
assert.ok(path, 'usage: node analyze-pairs.mjs /path/to/results.json');
const report = JSON.parse(await readFile(path, 'utf8'));
assert.ok(report.finished, 'benchmark must finish before paired analysis');
assert.ok(report.trials.length > 0 && report.trials.length % 2 === 0, 'complete adjacent pairs required');
const mean = values => values.reduce((a,b)=>a+b,0)/values.length;
const range = values => [Math.min(...values),Math.max(...values)];
const pairs = [];
for (let i=0;i<report.trials.length;i+=2) {
  const trials = report.trials.slice(i,i+2);
  assert.deepEqual(trials.map(t=>t.variant).sort(), ['chartjs','svg20']);
  const svg = trials.find(t=>t.variant==='svg20');
  const chart = trials.find(t=>t.variant==='chartjs');
  pairs.push({
    pair:pairs.length+1,order:trials.map(t=>t.variant),
    svg_cpu_percent:svg.cpu_percent,chartjs_cpu_percent:chart.cpu_percent,
    cpu_reduction_points:svg.cpu_percent-chart.cpu_percent,
    cpu_reduction_percent:100*(1-chart.cpu_percent/svg.cpu_percent),
    svg_paint_hz:svg.paint_hz,chartjs_paint_hz:chart.paint_hz,
    paint_cadence_ratio:chart.paint_hz/svg.paint_hz,
  });
}
const variants = Object.fromEntries(['svg20','chartjs'].map(variant=>{
  const trials=report.trials.filter(t=>t.variant===variant);
  const elapsed=trials.reduce((sum,t)=>sum+t.elapsed,0);
  return [variant,{
    trials:trials.length,measured_seconds:elapsed,
    cpu_percent:trials.reduce((sum,t)=>sum+t.cpu_percent*t.elapsed,0)/elapsed,
    cpu_percent_range:range(trials.map(t=>t.cpu_percent)),
    paint_hz:trials.reduce((sum,t)=>sum+t.paint_hz*t.elapsed,0)/elapsed,
    paint_gap_ms_p95_range:range(trials.map(t=>t.paint_gap_ms_p95)),
    paint_gap_ms_max:Math.max(...trials.map(t=>t.paint_gap_ms_max)),
    renderer_task_cpu_percent:mean(trials.map(t=>t.renderer_task_cpu_percent)),
    js_heap_before_mib:mean(trials.map(t=>t.js_heap_before_bytes/1048576)),
    js_heap_after_mib:mean(trials.map(t=>t.js_heap_bytes/1048576)),
    rss_mib_median_range:range(trials.map(t=>t.rss_mib_median)),
    max_observation_gap_seconds:Math.max(...trials.map(t=>t.max_observation_gap_seconds)),
  }];
}));
const lowerCpuPairs=pairs.filter(p=>p.cpu_reduction_points>0).length;
console.log(JSON.stringify({
  started:report.started,finished:report.finished,source:report.source,
  host:report.host,browserVersion:report.browserVersion,chartjsVersion:report.chartjsVersion,
  inputs:report.inputs,warmup:report.warmup,seconds:report.seconds,
  rejectedTrials:report.rejectedTrials??[],variants,pairs,
  lower_cpu_pairs:lowerCpuPairs,pair_count:pairs.length,
  mean_paired_cpu_reduction_percent:mean(pairs.map(p=>p.cpu_reduction_percent)),
  paired_cpu_reduction_percent_range:range(pairs.map(p=>p.cpu_reduction_percent)),
  consistent_observed_cpu_reduction:lowerCpuPairs===pairs.length,
  higher_paint_cadence_in_every_pair:pairs.every(p=>p.paint_cadence_ratio>1),
},null,2));

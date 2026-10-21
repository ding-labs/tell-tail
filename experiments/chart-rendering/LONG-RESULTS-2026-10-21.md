# Longer paired chart benchmarks

## Native Chart.js animation

Measured October 21, 2026, 11:12–11:32 America/Los_Angeles, on Apple M3,
macOS / Darwin 27.0.0, Chrome 156.0.8078.47 and Chart.js 4.5.1.
The exact host release, source revision `08b7754`, and SHA-256 hashes of the
renderer, adapter, workload and Chart.js bundle are in the local raw report.

**Native Chart.js used more CPU in all four pairs.** It provided a higher paint
cadence, but did not meet the goal of lowering CPU while doing so on this machine.

| Adjacent pair | Order | SVG CPU % | Chart.js CPU % | Chart.js CPU increase |
| --- | --- | ---: | ---: | ---: |
| 1 | SVG, Chart.js | 16.67 | 24.90 | 49.3% |
| 2 | Chart.js, SVG | 20.12 | 28.60 | 42.2% |
| 3 | SVG, Chart.js | 19.12 | 25.50 | 33.4% |
| 4 | Chart.js, SVG | 16.77 | 20.49 | 22.2% |

CPU is cumulative CPU time across the isolated browser process tree divided by
wall time; 100% means one CPU core. Each trial warmed up for 30 seconds, then
measured for 120 seconds. All eight trials used fresh headed browsers, the same
deterministic full history and the same visible 1280 × 1100 viewport. The driver
updated dashboard metrics every 100 ms and history every second, with full-second
animation. The plan measured 16 minutes of browser operation, plus four minutes
of warmup. Idle display/system sleep was inhibited during the campaign.

| Aggregate observation | Current SVG | Native Chart.js |
| --- | ---: | ---: |
| Duration-weighted CPU | 18.17% | 24.87% |
| CPU range across trials | 16.67–20.12% | 20.49–28.60% |
| Actual paints/sec | 18.51 | 60.00 |
| Paint-gap p95 across trials | 67.0–67.1 ms | 17.4–17.5 ms |
| Largest recorded paint gap | 70.9 ms | 25.9 ms |
| Mean renderer task CPU | 6.28% | 7.05% |
| Mean JS heap before measurement, post-GC | 1.45 MiB | 2.95 MiB |
| Mean JS heap after measurement, post-GC | 1.58 MiB | 3.16 MiB |
| Range of whole-browser median RSS | 734–833 MiB | 795–880 MiB |

Chart.js painted about 3.24 times as often, with approximately 37% higher total
CPU on average. These draw timings support higher animation cadence, not a
guarantee about GPU presentation or perceptual smoothness on every device.
Whole-browser RSS includes shared pages and Chrome infrastructure. The heap
observations do not constitute a long-duration leak certification.

The October 20 short run observed approximately 30 Hz native animation, whereas
this campaign observed 60 Hz. The native configuration is the same, but the actual
animation workload is therefore different. This result does not settle whether
an explicitly capped 30 Hz Chart.js variant saves CPU.

All accepted trials passed plot-edge alignment, finite-coordinate, intermediate
scrolling and freeze checks. Every resource sample had the expected process IDs
and a visible page. The maximum observation gap was 1.105 seconds; no measured
animation stalled, including at recording boundaries. No accepted trial needed
a process-tree retry. An earlier overlapping preflight was discarded entirely
before this campaign; a tested workspace lock then prevented concurrent runs.

This is a counterbalanced, four-pair observation on one machine with normal
desktop activity and unpinned CPU frequency. It supports the direction of the
effect under these conditions, rather than a universal claim about chart libraries.
The production chart remains unchanged.

Reproduction commands and method are in [README.md](README.md). Run
`analyze-pairs.mjs` against the final raw report to reproduce the paired table.
Local artifacts are `work/chart-benchmark-long/results.json`, `paired-summary.json`,
fixed-history screenshots in that directory, and `work/chart-benchmark-long-run.txt`.

## Explicit 30 Hz follow-up

Measured October 21, 11:33–11:43 America/Los_Angeles, with the same host,
browser, workload, geometry, 30-second warmup and 120-second measurement periods.
Source `ac7f693` adds only a benchmark variant and analysis support; production
rendering remains unchanged. Two adjacent pairs reversed the renderer order.

This variant uses Chart.js native full-second interpolation but caps Canvas
paints at 30 Hz. A 0.5 ms scheduling tolerance accommodates 33.3 ms callbacks on
the 60 Hz display. Native animation calculations still run at browser refresh
rate, so this evaluates a paint cap rather than a redesigned animation scheduler.

| Pair | Order | SVG CPU % | Chart.js CPU % | Chart.js CPU increase | SVG / Chart.js paints per second |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Chart.js 30 Hz, SVG | 16.86 | 18.82 | 11.6% | 18.82 / 29.18 |
| 2 | SVG, Chart.js 30 Hz | 17.21 | 24.07 | 39.8% | 18.71 / 28.66 |

Duration-weighted CPU was **17.04% for SVG and 21.44% for capped Chart.js**,
an approximately 26% increase. Actual paint cadence was 18.76 versus 28.92 Hz,
respectively: Chart.js produced about 1.54 times as many paints. Its median paint
gap was 33.3–33.4 ms and p95 was 49.2–49.8 ms, compared with SVG's approximately
50.1 ms median and 66.9–67.0 ms p95. The higher cadence remained, but neither
pair showed a total-browser CPU saving.

Mean renderer task CPU was 5.94% for SVG and 5.80% for Chart.js. That small
main-thread difference did not translate into lower CPU across the whole browser.
Mean post-GC JS heap grew from 1.44 to 1.55 MiB for SVG and from 2.92 to 3.12 MiB
for Chart.js over the measured phases. These observations do not establish leaks.

All four follow-up trials passed the same process, visibility, sampling-gap,
paint-gap, coordinate, scrolling and freeze checks. The largest observation gap
was 1.136 seconds. No trial was rejected. The workspace lock prevented overlapping
benchmark processes; all recorded benchmark browser PIDs were gone after both
campaigns, and neither the benchmark harness nor its lock remained active.
The machine was observed on battery power with Low Power Mode disabled; CPU
frequency and other desktop activity were not controlled.

In total, **all six paired comparisons used more CPU with Chart.js**. The combined
campaigns contain approximately 24 minutes of accepted measurement, plus warmup.
The 30 Hz follow-up has only two pairs, so it is less extensive than the four-pair
native study, but it also does not support a CPU-driven migration. A higher paint
cadence is a possible product benefit; lower CPU is not demonstrated by either
configuration. Keep the current production renderer for the present CPU goal.

Follow-up artifacts are under `work/chart-benchmark-long-30hz/`, including
`results.json`, `paired-summary.json`, screenshots and power observations. The
progress log is `work/chart-benchmark-long-30hz-run.txt`. Reproduce the summary
with `analyze-pairs.mjs work/chart-benchmark-long-30hz/results.json chartjs30`.

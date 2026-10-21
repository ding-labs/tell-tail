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

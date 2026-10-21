# Optimized SVG dashboard: real Apple-log soak

The optimized SVG dashboard averaged **13.89% of one CPU core** in valid
post-warmup browser blocks. Tell-Tail averaged **0.97%**, and Apple's capture
process **2.25%**. Backend memory stayed near 3 MiB. Browser memory showed modest
growth: retained JavaScript heap increased 46 KiB over 15 minutes and leveled off
at the final checkpoint; whole-browser RSS minute medians increased about 14 MiB.
No leak was established, but the browser RSS increase remains unattributed.

Measured October 21, 2026, 12:49–13:07 America/Los_Angeles, on Apple M3,
macOS 27.0 (26A428), Darwin 27.0.0, headed Chrome 156.0.8078.47. The approximately
18.6-minute session included two minutes of warmup, five three-minute live
blocks, 30 seconds frozen, and one minute resumed. It processed **305,410 real
Apple log records / 309 MiB** overall. The 15-minute steady measurement processed
234,115 records / 235 MiB, averaging 260 records/sec. Block averages ranged from
153 to 431 records/sec; real system activity was not controlled.

## CPU and memory

| Process group | Live CPU, % of one core | Median RSS, MiB | Live RSS range, MiB |
| --- | ---: | ---: | ---: |
| Tell-Tail | 0.97% | 2.98 | 2.94–3.28 |
| Apple `log stream` | 2.25% | 4.73 | 4.55–5.02 |
| Entire isolated Chrome instance | 13.89% | 787.45 | 763.73–916.50 |

CPU is cumulative process CPU time divided by wall time; 100% means one core.
Tell-Tail/capture CPU covers 901.8 steady-state seconds. Chrome CPU covers the
last four blocks (721.7 seconds): a short-lived child process appeared and exited
in the first block, invalidating its cumulative browser CPU total. That block is
excluded from the CPU mean, not silently treated as a lower CPU observation. All
15 minutes remain valid for memory observations. No later process changes occurred.

| Live block | Records/sec | Tell-Tail CPU | Capture CPU | Chrome CPU | Chrome median RSS, MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 344 | 1.01% | 2.58% | Excluded | 783.17 |
| 2 | 195 | 0.94% | 1.98% | 14.34% | 785.28 |
| 3 | 431 | 1.04% | 3.01% | 12.64% | 784.31 |
| 4 | 177 | 0.94% | 1.92% | 14.35% | 787.59 |
| 5 | 153 | 0.91% | 1.76% | 14.22% | 792.41 |

Freezing the chart while capture continued reduced Chrome CPU to **3.09%**;
resuming returned it to **14.42%**. Median browser RSS was 794.36 MiB frozen and
789.64 MiB resumed. These are short diagnostic phases with varying real input,
not paired fixed-workload trials.

The Chrome RSS total sums browser, renderer, GPU and utility processes and can
double-count shared pages. It is not tab-private memory or JavaScript heap. No
blank-browser control was measured, so it does not isolate incremental tab cost.
This real-stream run is not a before/after CPU comparison with the older renderer.

## Memory growth checks

CDP forced garbage collection outside the CPU measurement blocks, then sampled:

| Post-warmup live time | Used JS heap, MiB | Embedder heap, MiB | DOM nodes | Listeners | Chart dots / table rows | Tell-Tail FDs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 minutes | 1.802 | 1.344 | 874 | 23 | 60 / 60 | 10 |
| 3 minutes | 1.816 | 0.937 | 874 | 23 | 60 / 60 | 10 |
| 6 minutes | 1.831 | 0.946 | 874 | 23 | 60 / 60 | 10 |
| 9 minutes | 1.838 | 0.935 | 874 | 23 | 60 / 60 | 10 |
| 12 minutes | 1.848 | 0.954 | 874 | 23 | 60 / 60 | 10 |
| 15 minutes | 1.847 | 0.941 | 874 | 23 | 60 / 60 | 10 |

Retained JS heap grew **47,092 bytes / 46.0 KiB / 2.5%** from warmup, with a
1,408-byte decrease in the final three-minute interval. Its fitted slope across
all six checkpoints was +3.20 KiB/min, dominated by the earlier increase; this
is not an established ongoing leak rate. Heap capacity decreased from 2.563 to
2.063 MiB, embedder heap decreased, and backing storage stayed near 25.2 KiB.
After freeze/resume, used heap was 1.852 MiB, with the same live DOM/listener counts.

All checkpoints had one document, 480 elements, 60 chart dots and 60 table rows.
Live paths stayed at 60–61 cubic segments; freezing settled them to 59. The
interruption-related geometry bound therefore held under real log streaming.
Tell-Tail file descriptors remained at 10.

RSS trends use minute medians, excluding bins spanning less than 45 seconds.
Tell-Tail's first/last minute medians were 3.172 / 2.984 MiB and Apple's were
4.734 / 4.719 MiB; fitted slopes were effectively flat (+0.001 / −0.0004 MiB/min).
Chrome's were **779.875 / 793.734 MiB**, a **13.86 MiB increase**, with a fitted
slope of +0.76 MiB/min. The brief 916.5 MiB spike coincided with the extra browser
process and receded. The remaining RSS rise could include native browser caches
or allocations; JS/DOM observations cannot explain or exclude a browser-native
leak. This run does not justify calling all browser memory flat or leak-free.

A final macOS `leaks` scan reported **0 leaks / 0 leaked bytes** for Tell-Tail,
385 malloc nodes / 232 KiB, and a 2,624 KiB physical footprint. It also warned that
the process was not debuggable and inspection was restricted. This scan concerns
Tell-Tail, not Chrome, and is supporting evidence rather than exhaustive proof.
A multi-hour run would be needed to establish whether the modest browser RSS
increase plateaus or persists.

## Method and verification

The release binary ran `--apple-unified --output web`, owning
`/usr/bin/log stream --style ndjson --level info --type log`. Chrome used a fresh
profile, one page, and a 1280 × 1100 viewport at device scale 1. The chart stayed
visible, the connection stayed live, and no page errors occurred. Resource
sampling was approximately once per second, with a lightweight visibility and
connection check; a gap over five seconds would have aborted the study.
`caffeinate -di` inhibited idle/display sleep, and a workspace lock prevented
concurrent chart benchmarks. Periodic forced GC affects heap/cache behavior, so
this measures retained memory under those checks, not completely uninstrumented
browser operation. Raw log records were not saved.

Final ingestion diagnostics had zero parse failures, zero oversized records and
44 skipped bytes. All recorded browser, monitor and capture PIDs were absent
after normal shutdown; the benchmark lock was removed. The release build and a
short live harness smoke check passed. Summary totals, weighted CPU, heap deltas,
cleanup reporting and exclusion of partial RSS minutes were checked against the
saved observations. Application runtime code was unchanged during this audit.

Measured source: `6396245bd7a7cca26a661a2378cb3fe79f5d8905`.
Release binary SHA-256:
`4b0e770040fdf61e5db44c3970e9f2f4aaeba52f326bb62db3e51913b2d1e60a`.

Reproduction and measurement controls are in
[experiments/resource-audit/README.md](../experiments/resource-audit/README.md).
Local ignored artifacts are `work/live-browser-soak-2026-10-21/results.json`,
`summary.json`, `dashboard.png`, and `work/live-browser-soak-2026-10-21-log.txt`.

Follow-up: the [Chrome memory investigation](CHROME-MEMORY-INVESTIGATION-2026-10-21.md)
found evidence favoring normal working-set/cache retention, with bounded retained
page objects and reversible graphics footprints. It did not identify an
application leak or assign every byte of this run's RSS increase to one cache.

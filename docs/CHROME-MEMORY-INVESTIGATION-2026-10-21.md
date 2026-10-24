# Chrome memory-growth investigation

**The evidence favors normal Chrome working-set/cache and allocator behavior,
rather than an identified Tell-Tail memory leak.** The earlier 14 MiB RSS rise did
not repeat as sustained growth. In this follow-up, Chrome reclaimed substantial
RSS while the dashboard stayed live; retained page objects stayed bounded, and
navigation released the dashboard's objects. No production fix is justified by
these observations.

This does not assign every byte of the earlier RSS increase to a specific cache,
or rule out every slow browser-native leak. The critical-pressure control was
inconclusive. The stronger evidence is spontaneous reclamation, the retained
object census, reversible graphics footprints, and teardown/reopen behavior.

## Run and controls

Measured October 21, 2026, 13:31–13:58 America/Los_Angeles: **26.3 minutes and
398,920 real Apple log records**, on the same Apple M3/macOS 27.0 system and
headed Chrome 156.0.8078.47. Tell-Tail owned a real Apple unified-log capture;
Chrome used a fresh profile with the chart visible at 1280 × 1100, scale 1.

The first section used two-minute warmup, 30-second settling, three five-minute
live blocks, critical-pressure notification with 30 seconds live, three-minute
recovery, then 30 seconds frozen and 30 resumed. OS RSS was sampled every five
seconds. GC, heap usage, DOM and visibility checks occurred only at boundaries,
rather than evaluating the page every second as in the previous audit.

**No heap snapshots or Chrome tracing ran during this natural-growth section.**
Only afterward did the harness take two snapshots two minutes apart, collect
native memory dumps, navigate to `about:blank`, close the page, send pressure
again, and reopen the dashboard. Apple capture continued throughout. No raw log
records were saved. Sleep inhibition and the shared benchmark lock were active.

## Natural growth: RSS is not retained objects

RSS below is whole-browser summed RSS immediately after the lightweight GC check;
it includes shared resident pages and is not tab-private memory.

| Checkpoint | Chrome RSS, MiB | Retained JS heap, MiB | DOM nodes | Listeners |
| --- | ---: | ---: | ---: | ---: |
| Settled baseline | 859.98 | 1.623 | 874 | 23 |
| 5 minutes live | 865.69 | 1.626 | 874 | 23 |
| 10 minutes live | 754.13 | 1.633 | 874 | 23 |
| 15 minutes live | 743.08 | 1.638 | 874 | 23 |
| After critical pressure, still live | 750.48 | 1.637 | 874 | 23 |
| After three-minute recovery | 748.66 | 1.621 | 874 | 23 |
| Frozen | 749.98 | 1.624 | 874 | 22 |
| Resumed | 754.64 | 1.637 | 874 | 23 |

Retained JS grew **15,724 bytes / 15.4 KiB** over the 15-minute live section, then
returned **1,516 bytes below** the settled baseline after recovery. This does not
show persistent JS growth. The previous run's 46 KiB drift also did not reproduce
at the same rate with fewer in-page probes; instrumentation is a possible
contributor, not a proven attribution for that difference.

Live-block median RSS was 869.38, 802.38 and 755.97 MiB. The same browser processes
remained present through these natural phases. At an early sample, most of the
initial increase was in renderer PID 19495 (+8.34 MiB), with only +0.59 MiB in
the GPU process. Later, RSS receded across several processes. The settled-to-final
live RSS change was **−116.91 MiB**. A finite RSS upswing alone was therefore a
poor leak signal on this machine.

Critical pressure did not produce a large further RSS reduction: it went from
743.08 to 750.48 MiB at the next checkpoint. We do **not** use this as proof that
all growing memory was discardable cache.

## Kernel footprints and graphics resources

A separate `/usr/bin/footprint` observer recorded each owned Chrome process every
30 seconds, without starting Chrome tracing. Collection began about eight minutes
into the live measurement, so it cannot supply a footprint baseline for the first
eight minutes. All 32 observations completed without warnings or errors.
Apple's footprint accounting includes dirty and compressed memory, unlike a
simple resident-page total. See [Apple's memory diagnosis guidance](https://developer.apple.com/videos/play/wwdc2021/10180/).

| Phase | Samples | Total footprint median, MiB | Dashboard renderer median, MiB | GPU median, MiB |
| --- | ---: | ---: | ---: | ---: |
| Available portion of live block 2 | 5 | 575.26 | 80.27 | 311.14 |
| Live block 3 | 10 | 578.85 | 83.27 | 310.89 |
| Pressure, still live | 1 | 575.43 | 82.24 | 308.35 |
| Recovery live | 6 | 617.38 | 84.85 | 348.69 |
| Frozen | 1 | 475.59 | 80.25 | 210.28 |
| Resumed | 1 | 610.20 | 85.03 | 340.24 |

The active renderer's footprint varied by a few MiB rather than tracking the
large whole-browser RSS swings. GPU footprint dropped about **138 MiB** on freeze
and rebounded on resume while the DOM remained loaded. These short controls have
only one sample each, but the reversible behavior supports graphics working
resources rather than permanently retained old chart objects.

## Retained-object comparison and teardown

Two full heap snapshots were taken after the natural controls, with another
121 seconds of live rendering between them. They include native Blink objects
as well as V8 objects; their shallow-byte totals are not the Runtime JS heap size.

| Retained object | First snapshot | Second snapshot | Blank page | Reopened dashboard |
| --- | ---: | ---: | ---: | ---: |
| Native chart circles | 60 | 60 | 0 | 60 |
| Native Animation | 1 | 1 | 0 | 1 |
| Native KeyframeEffect | 1 | 1 | 0 | 1 |
| Native EventSource | 1 | 1 | 0 | 1 |
| Explicitly detached DOM seeds | 0 | 0 | 0 | 0 |
| JS Array objects | 61 | 61 | 3 | 61 |

Total snapshot node count fell from **52,460 to 52,451**. There were 41,972 common
IDs, 10,479 new IDs and 10,488 removed IDs: replacement without accumulation.
Shallow bytes increased by 17,408, led by a **16,384-byte increase in one Blink
matched-properties cache backing table**, from 16,392 to 32,776 bytes. The cache
entry-object count stayed at **153**, and smaller changes included compiled code
and layout metadata. This is cache-table capacity growth, not an increasing
population of application history/animation objects.

The [matching Chromium 156.0.8078.47 source](https://github.com/chromium/chromium/blob/156.0.8078.47/third_party/blink/renderer/core/css/resolver/matched_properties_cache.cc)
implements cleanup/LRU pruning above 500 entries, targeting 300. This is a cleanup
policy, not a claim that transient entry counts or table capacity can never exceed
those numbers. We did not force that cache to its pruning threshold in this run.

On `about:blank`, DOM counters fell to **4 nodes / 0 listeners** and all listed
dashboard objects disappeared. On page closure, all three old renderer PIDs
exited. Kernel-accounted whole-browser footprint fell to **208.66 MiB**, including
the still-running tracing service; GPU footprint was **51.61 MiB**. Reopening
created new renderers, with the original object counts and 1.611 MiB JS heap.
Teardown alone would not exclude a page-lifetime leak, but it complements the
bounded live snapshot comparison.

## Profiling overhead and limits

The first heavy checkpoint raised RSS from **754.64 to 1,212.86 MiB**, a
**458.22 MiB profiling disturbance**, and started a tracing-service process.
Later heavy observations are excluded from natural-growth RSS comparisons.
Preflight runs detected this overhead before the accepted campaign.

Native dumps named Metal/IOSurface/Skia resources and other process allocators.
Parent/child allocator paths overlap and were not added together. The dump
succeeded globally but omitted the active renderer's allocator subtree, so it
cannot pinpoint every renderer-native allocation. Kernel footprints and heap
snapshots supplied independent observations. An exploratory `vmmap` malloc-zone
summary warned that its analysis was incomplete; no conclusion depends on that
malloc-zone breakdown.

Normal desktop activity, OS memory management, periodically forced GC and
profiling affect the results. This is one controlled diagnostic sequence, not a
multi-hour or multi-device guarantee. The appropriate conclusion is **no identified
application leak, with evidence favoring normal Chrome resource/cache retention**,
rather than a certification that all native allocations are leak-free.

## Verification and artifacts

No page errors or visibility/connection failures occurred. All recorded browser,
tracing, renderer, monitor and capture PIDs were absent after shutdown; both the
main harness and footprint observer exited, and the lock was removed. Application
runtime code was unchanged. Smoke checks exercised pressure, snapshot capture,
native dumps, blank navigation, closure/reopening and cleanup. Analyzer checks
covered identity/diff arithmetic, teardown object removal, hexadecimal native
scalars, per-process totals and timestamp-based footprint phase assignment.

Measured source: `9fc46a90ad20030253f7c9e4d7fe90ae258b81cf`.
Release binary SHA-256:
`4b0e770040fdf61e5db44c3970e9f2f4aaeba52f326bb62db3e51913b2d1e60a`.

Reproduction and primary method references are in
[experiments/resource-audit/README.md](../experiments/resource-audit/README.md).
Ignored local artifacts live under `work/chrome-memory-2026-10-21/`: `results.json`,
`summary.json`, `footprint-observations.json`, `footprint-summary.json`,
`heap-comparison.json`, `native-comparison.json`, snapshots and native dumps.
Progress is in `work/chrome-memory-2026-10-21-log.txt`. Earlier smoke runs are
separate and are not included in these findings.

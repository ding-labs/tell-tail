# SVG scrolling optimization

The production renderer remains SVG. On this Apple M3, moving its plot with a
native transform reduced whole-browser CPU from **20.52% to 14.15% of one core**
(31.0%) across four counterbalanced pairs. Every pair favored the new renderer.
Separately sampled visible scrolling increased from **17.33 to 60.16 updates/sec**.

| Pair | Order | Previous SVG CPU | Optimized SVG CPU | Reduction |
| --- | --- | ---: | ---: | ---: |
| 1 | Optimized, previous | 19.63% | 14.72% | 25.0% |
| 2 | Previous, optimized | 21.42% | 14.57% | 32.0% |
| 3 | Optimized, previous | 20.78% | 15.37% | 26.0% |
| 4 | Previous, optimized | 20.23% | 11.95% | 41.0% |

## Change

The grid and axes stay fixed. The line, fill and dots share an HTML layer holding
an SVG. When retained values and their scale are unchanged, geometry is updated
once per publication and a full-second Web Animations transform scrolls the layer.
Chrome's LayerTree diagnostics confirmed `ActiveTransformAnimation` and
`WillChangeTransform` compositing reasons on the actual embedded dashboard.

Scale changes and revised historical values retain the existing 20 Hz geometry
interpolation. A publication arriving within its final paint interval settles
that transition so tiny residual y-errors cannot prevent subsequent native
scrolling. An interrupted native scroll starts from its current visible position.
Percentage transforms preserve positioning across responsive resizing.

The plot retains at most two outgoing vertices in addition to its 60-point
window. A regression test repeatedly interrupts 39 scrolls before completion:
without the bound the path grew to 98 cubic segments; with it the path stays at
61 or fewer. This removes accumulating offscreen geometry, without claiming that
all possible application memory leaks have been excluded.

## Measurement

Measured October 21, 2026, 12:01–12:14 America/Los_Angeles; Apple M3, arm64,
Darwin 27.0.0, Chrome 156.0.8078.47, 1280 × 1100 viewport at device scale 1.
Each of eight fresh headed-browser trials warmed up for 30 seconds and measured
CPU for 60 seconds: eight measured minutes, plus four warmup minutes. The
benchmark lock prevented overlapping runs; `caffeinate -di` inhibited sleep.
The previous SVG is pinned to `cca36ad82271f7827953d5519514c1825dd67255`;
the optimized source is `b65411985cd1b51299d525e6e426b5ed5c68da44`.
The geometry bound was added afterward in `0752fcf` and checked separately below.

CPU is the delta in cumulative process CPU time over wall time for the entire
isolated browser process tree; 100% means one core. The deterministic workload
publishes dashboard metrics every 100 ms and a completed history point every
second. It exercises the real renderer directly, excluding native capture and
SSE transport. Absolute CPU is therefore different from a live Apple-log audit.

| Observation | Previous SVG | Optimized SVG |
| --- | ---: | ---: |
| Duration-weighted CPU | 20.52% | 14.15% |
| Mean renderer task CPU | 6.89% | 2.97% |
| Geometry updates/sec | 18.31 | 1.59 |
| Visible motion updates/sec | 17.33 | 60.16 |
| Visible motion gap p95 range | 68.5–83.9 ms | 17.3–18.7 ms |
| Mean post-GC heap before / after | 1.428 / 1.506 MiB | 1.451 / 1.493 MiB |
| Whole-browser median RSS range | 897–912 MiB | 858–877 MiB |

Visible motion is sampled by tracking a retained dot's screen position during a
separate four-second rAF probe after each CPU measurement. Keeping these layout
reads outside the CPU phase avoids distorting compositor savings. It measures
observed positions, not GPU presentation or dropped frames. Geometry-update gaps
of about a second in the optimized path are expected and are not animation stalls.
Scale-changing transitions still use the original interpolation cadence.

All eight trials passed coordinate, plot-edge, intermediate-scrolling and freeze
checks, retained stable browser process trees and visible pages, and had resource
observation gaps no greater than 1.081 seconds. No accepted trial was rejected or
retried. Fixed-history screenshots were visually inspected for curve, fill, dot,
grid and label parity. Exploratory measurements and an earlier campaign exposing
the scale-transition handoff issue were excluded from these results.

This is a four-pair study on one machine with normal desktop activity and unpinned
CPU frequency. It supports this implementation under these conditions, rather
than a universal performance guarantee. Heap and RSS measurements are short-term
observations, not a long-duration leak certification.

## Verification and reproduction

The final geometry-bound build passes all 19 Chrome browser/Node tests, including
actual SSE updates, responsive layout, bounded curves, interruption continuity,
scale transitions, freeze, reduced motion, session reset, final state, reconnection
and a native hidden-tab test. Both targeted chart tests pass in cached Playwright
WebKit. Cached Firefox exited during browser launch with “Could not find profile
folder”; no Firefox application behavior was validated. All 10 Rust `web::` tests
pass and the release build succeeds.

Raw results, per-second samples, input SHA-256 hashes, screenshots and paired
summary are in ignored local `work/svg-motion/paired/`; progress is recorded in
`work/svg-motion/paired-run.txt`. The compositor evidence is in
`work/svg-motion/compositing.json`, with test logs alongside it.

```sh
CHART_BENCH_SECONDS=60 CHART_BENCH_WARMUP=30 \
CHART_BENCH_ORDER=svg-motion,svg20,svg20,svg-motion,svg-motion,svg20,svg20,svg-motion \
CHART_BENCH_OUTPUT="$PWD/work/svg-motion/paired" \
caffeinate -di node experiments/chart-rendering/benchmark.mjs
node experiments/chart-rendering/analyze-pairs.mjs work/svg-motion/paired/results.json svg-motion
```

The harness reads current workspace assets for `svg-motion` and captures them at
startup. Its `paint_hz` means SVG path mutations, so use `visible_motion_hz` when
assessing transform animation. See [README.md](README.md) for the full method.

## Final bounded-version confirmation

After `0752fcf`, a fresh previous/optimized pair with 10-second warmups and
30-second CPU phases measured **18.91% versus 16.01% CPU** (15.4% lower).
Sampled motion was 17.48 versus 60.11 updates/sec, and geometry updates were
17.95 versus 1.00/sec. Both trials passed the same correctness and process checks.
Artifacts are in `work/svg-motion/final-confirmation/`. This shorter confirmation
is reported separately from the four-pair campaign: it confirms the direction on
the final bound, but does not replace the longer estimate or eliminate desktop
variability. All five observed pairs favored the optimized renderer.

# Browser chart comparison

Benchmark-only Chart.js 4.5.1 adapter and repeatable workload. Production assets
and Rust dependencies are unchanged. The adapter preserves the dashboard, SVG
grid/labels and exact-value table while replacing the plot with a Canvas layer.
This is not a complete replacement: production resizing, accessibility, tooltip,
visibility/reconnect and bundling integration would need separate validation.

From the repository root, with Node, browser-tests dependencies and Chrome installed:

```sh
npm ci --prefix experiments/chart-rendering
node experiments/chart-rendering/benchmark.mjs
```

Results and screenshots go to ignored `work/chart-benchmark/`. To smoke-test:

```sh
CHART_BENCH_SECONDS=2 CHART_BENCH_WARMUP=2 CHART_BENCH_ORDER=svg20,chartjs20,chartjs,svg60 node experiments/chart-rendering/benchmark.mjs
```

The default run uses eight independent headed Chrome instances, one at a time,
in mirrored order: SVG20, Chart.js20, Chart.js native, SVG uncapped, then reverse.
Each trial warms up for five seconds with a full history and measures 30 seconds.
`CHART_BENCH_SECONDS`, `CHART_BENCH_WARMUP`, `CHART_BENCH_ORDER`,
`CHART_BENCH_OUTPUT` and `CHART_BENCH_CHANNEL` override these settings.
An unstable browser process tree invalidates the trial and triggers up to two
fresh-browser retries. `CHART_BENCH_RESUME=1` appends trials to an existing report;
specify the remaining variants with `CHART_BENCH_ORDER` when resuming.
The existing `PLAYWRIGHT_MODULE_DIR` override can select a Playwright installation.
A workspace lock under `work/.chart-rendering-benchmark-lock/` prevents overlapping
benchmark processes, including runs with different output directories. If a forced
kill leaves a stale lock, verify that its recorded PID is no longer running before
removing the lock directory. Playwright can handle SIGTERM without terminating
the harness; confirm process exit before starting a replacement run.

For four longer pairs, alternating which renderer runs first:

```sh
CHART_BENCH_SECONDS=120 CHART_BENCH_WARMUP=30 \
CHART_BENCH_ORDER=svg20,chartjs,chartjs,svg20,svg20,chartjs,chartjs,svg20 \
CHART_BENCH_OUTPUT="$PWD/work/chart-benchmark-long" \
caffeinate -di node experiments/chart-rendering/benchmark.mjs
```

`caffeinate -di` on macOS prevents display and idle system sleep while the command runs.
On other systems, run Node directly. This plan measures eight trials for about
20 minutes including warmup. It records input hashes, host information, per-second
CPU/RSS samples, pre/post-GC heap sizes, maximum paint gaps, and minute progress.
Every sample checks process IDs and page visibility; an observation gap of five
seconds or an animation stall of one second invalidates the run instead of
counting suspended time as evidence. Only process-tree changes trigger automatic
retries; other failures require inspecting the cause before resuming.

Once that paired run completes, produce a summary with:

```sh
node experiments/chart-rendering/analyze-pairs.mjs work/chart-benchmark-long/results.json
```

The analysis keeps each adjacent SVG/Chart.js pair together and reports every
pair's relative CPU change and paint-cadence ratio. A lower overall mean alone
does not establish consistency; inspect how many individual pairs favor Chart.js
and the size of their differences. Four pairs on one machine are still a small
sample, not a general claim about all browsers or devices.

All variants show the same deterministic 60-point history, monotone curves, dots,
fill, axes and exact labels. Dashboard metrics update every 100 ms; the history
advances once per second. Animation duration is always 1,000 ms. The viewport is
1280 × 1100 at device scale 1 so the whole chart is visible. The driver calls the
existing renderer directly; no native log capture, SSE parsing or reconnection
logic is involved. Absolute CPU therefore differs from a live capture audit.

- `svg20`: unmodified production chart with a 20 Hz paint ceiling.
- `svg60`: production chart with that ceiling removed (actual refresh rate varies).
- `chartjs`: native Chart.js animation at browser refresh rate; parsing disabled,
  sorted/normalized data, explicit scale bounds, no legend/tooltip/event handling.
- `chartjs20`: the same native animation with a `beforeDraw` plugin limiting Canvas
  paints to 20 Hz. Animation calculations still run at browser refresh rate; this
  separates paint cost from scheduling. Completion and stop force a final paint.
- `chartjs30`: the same native animation with a 30 Hz paint ceiling, to test the
  earlier observed smoother cadence on a display now delivering 60 Hz. A 0.5 ms
  scheduling tolerance avoids rejecting a 33.3 ms interval because it is just
  below 1,000/30. Other variants' scheduling rules are unchanged. Pass `chartjs30`
  as a second argument to `analyze-pairs.mjs` for pairs using this variant.

The two-pair 30 Hz follow-up uses:

```sh
CHART_BENCH_SECONDS=120 CHART_BENCH_WARMUP=30 \
CHART_BENCH_ORDER=chartjs30,svg20,svg20,chartjs30 \
CHART_BENCH_OUTPUT="$PWD/work/chart-benchmark-long-30hz" \
caffeinate -di node experiments/chart-rendering/benchmark.mjs
node experiments/chart-rendering/analyze-pairs.mjs work/chart-benchmark-long-30hz/results.json chartjs30
```

CPU is the delta in cumulative CPU time for the entire isolated browser process
tree, sampled with `ps`; 100% is one core. A process-tree stability assertion
guards against losing CPU time from exiting children. RSS sums the whole browser
tree and may count shared pages more than once. CDP records renderer task time,
post-GC JS heap and DOM counts. Actual paint cadence uses SVG path mutations or
Chart.js `afterDraw`, not just requestAnimationFrame callback counts. Paint gaps
include update boundaries and are not a GPU presentation/drop-frame measurement.

Every trial checks that a retained second moves across three intermediate
positions throughout its animation, point coordinates remain finite, both plot
edges align with the original SVG, stopping freezes geometry, and no uncaught
page errors occur. Screenshots at the same fixed history support visual review.
Two trials per renderer provide an initial comparison, not a broad device study.
The version's official guidance is available in the [performance documentation](https://www.chartjs.org/docs/latest/general/performance.html)
and [animation documentation](https://www.chartjs.org/docs/latest/configuration/animations.html).

# Live Apple-log browser soak

Runs the release Tell-Tail dashboard against real macOS unified logs in an
isolated headed Chrome instance. No raw log records are persisted: results contain
aggregate application metrics and process/resource observations.

Build and run from the repository root with installed Chrome and the
`browser-tests` Playwright dependencies:

```sh
cargo build --release --locked
TELL_TAIL_BIN=target/release/tell-tail \
SOAK_OUTPUT="$PWD/work/live-browser-soak" \
caffeinate -di node experiments/resource-audit/live-browser.mjs
node experiments/resource-audit/summarize.mjs work/live-browser-soak/results.json
```

On macOS, `caffeinate -di` prevents idle system/display sleep. The default is a
120-second warmup, five 180-second live phases, 30 seconds frozen while ingestion
continues, and 60 seconds resumed. These can be changed with `SOAK_WARMUP`,
`SOAK_SECONDS`, `SOAK_BLOCK`, `SOAK_FROZEN`, and `SOAK_RESUMED`. A short plumbing
check uses values 2, 3, 3, 1, and 1 respectively; it is not performance evidence.
`PLAYWRIGHT_MODULE_DIR` selects a Playwright installation when necessary.

The harness samples RSS and cumulative process CPU time approximately every
second for Tell-Tail, its Apple capture child, and the entire isolated browser
process tree. The chart must remain visible and connected. A missing process or
resource observation gap over five seconds aborts the study. Changes in a group's
process IDs invalidate its CPU result for that phase; they are recorded rather
than silently dropping terminated children's CPU time. The same workspace lock
as the chart benchmarks prevents overlapping campaigns.

CPU is process CPU time divided by wall time; 100% means one core. RSS sums
process resident sets, so the browser total can double-count shared pages. It is
neither tab-private memory nor JavaScript heap. The summary fits RSS slopes to
per-minute medians of the post-warmup live samples and separately reports the
first/last minute medians. Slopes describe a finite observation, not proof of a
leak or its absence.

After warmup and each live phase, CDP requests garbage collection, then records
used JavaScript heap, embedder heap, backing storage, DOM/listener counts, chart
point/table/path counts, and monitor file descriptors. Heap observations and
screenshots are outside CPU phases. One lightweight connection/visibility check
per second adds small browser instrumentation overhead. Buttons are triggered
with DOM clicks to avoid Playwright locator-click instrumentation in subsequent
heap comparisons. The final macOS `leaks` result retains any inspection warnings;
a restricted scan is supporting evidence only.

The source revision and release binary SHA-256 are saved in `results.json`.
`dashboard.png` shows initial layout; the results verify full chart history after
warmup. Progress is flushed every 30 samples. CPU phase results exclude warmup
and forced-GC pauses when summarizing the live phases. The harness closes its
owned browser, monitor and Apple capture process, checks recorded PIDs, and
removes its lock on completion. If a forced kill leaves a lock behind, verify
its recorded PID is gone before removing it. Playwright can intercept SIGTERM;
confirm actual process exit before launching a replacement.

A long soak can reveal sustained growth, but real log rates and system load
vary. Compare measured phases within their workload context. A 15-minute clean
run cannot exclude small, infrequent or workload-specific leaks.

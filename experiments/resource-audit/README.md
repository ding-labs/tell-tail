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

## Investigating Chrome growth

`chrome-memory.mjs` separates natural RSS growth from heavyweight profiling. It
keeps real Apple capture running and records each Chrome PID and process role.
The live phases use OS RSS samples every five seconds and lightweight GC/DOM/heap
checks only at phase boundaries. No tracing or heap snapshots run during those
natural-growth and memory-pressure controls.

```sh
TELL_TAIL_BIN=target/release/tell-tail \
MEMORY_CONTROL=30 MEMORY_RECOVERY=180 \
MEMORY_OUTPUT="$PWD/work/chrome-memory" \
caffeinate -di node experiments/resource-audit/chrome-memory.mjs
node experiments/resource-audit/summarize-chrome-memory.mjs work/chrome-memory/results.json
node experiments/resource-audit/inspect-heap.mjs \
  work/chrome-memory/profile-before.heapsnapshot \
  work/chrome-memory/profile-after.heapsnapshot \
  work/chrome-memory/blank.heapsnapshot
node experiments/resource-audit/inspect-native.mjs work/chrome-memory/profile-after.memory.json
```

These settings take about 26 minutes: two-minute warmup, 30-second settling,
three five-minute live blocks, memory pressure with 30 seconds live, three-minute
recovery, 30 seconds frozen, and 30 seconds resumed. Heavy profiling then records
a heap/native baseline, runs another two minutes live, takes comparison snapshots,
and tests navigation to a blank page, page closure, closed-page pressure, and
reopening. Each final control settles for 30 seconds. `MEMORY_WARMUP`,
`MEMORY_BLOCK`, `MEMORY_BLOCKS`, `MEMORY_RECOVERY`, `MEMORY_CONTROL`, and
`MEMORY_PROFILE` configure the durations. For a plumbing check, set every value
to 1 or 2 and use a separate output directory; it is not performance evidence.

`Memory.simulatePressureNotification` sends critical pressure to this isolated
Chrome instance; it does not impose system-wide memory pressure. Reclamation
while the live chart retains its expected state supports discardable-cache or
allocator retention as an explanation. Reclamation only at navigation/closure
is weaker evidence: a page-lifetime leak can also disappear on teardown.
Persistent growth in reachable objects or detached DOM nodes warrants tracing
retainers rather than labeling all RSS growth a cache.

Heavy profiling has material overhead. A preflight observed hundreds of MiB of
additional RSS, a new tracing-service process, and renderer snapshot workspaces.
Every checkpoint records RSS immediately before/after diagnostics, and the
summary labels heavy checkpoints. Never combine those with natural-growth RSS.
Allocator paths in native memory dumps overlap; `inspect-native.mjs` reports them
individually without adding parent and child byte counts. The heap inspector
reports shallow object bytes/counts and preserved snapshot IDs, not dominator
retained sizes. It decodes the low detachedness bits using DevTools DOMLinkState
and reports explicitly detached seeds. An empty seed list alone is not a complete
leak proof.

Relevant primary references: [Chrome memory diagnosis](https://developer.chrome.com/docs/devtools/memory-problems),
[CDP Memory](https://chromedevtools.github.io/devtools-protocol/tot/Memory/), and
[CDP Tracing](https://chromedevtools.github.io/devtools-protocol/tot/Tracing/).

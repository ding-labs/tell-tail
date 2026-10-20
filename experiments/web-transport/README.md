# Web transport experiment

Development-only SSE/WebSocket comparison for [the local UI plan](../../docs/PLAN-WEB.md).
This isolated Cargo workspace reuses Tell-Tail's library without adding web
dependencies, routes, or assets to the release binary/package.

Run Rust checks from the repository root:

```sh
cargo test --manifest-path experiments/web-transport/Cargo.toml --locked
cargo clippy --manifest-path experiments/web-transport/Cargo.toml --locked --all-targets -- -D warnings
cargo build --release --manifest-path experiments/web-transport/Cargo.toml --locked
node --test experiments/web-transport/server.test.mjs
```

Run `experiments/web-transport/target/release/tell-tail-web-transport` to start a
loopback server on a random port. Its first stdout line is machine-readable JSON
with the URL and PID. `/events` and `/ws` carry identical frames; `/clock` exposes
the server monotonic clock for calibration, and `/status` reports current state.
`--hz 10|50|100`, `--profile representative|large-counters`, and
`--source fixture|stdin|file` select a workload. File input also needs
`--input-file PATH` and follows from the start. Actual ingestion still publishes
at its existing cadence; sampling it at 100 Hz does not produce 100 new snapshots.
`--padding-bytes` is exclusively for forcing socket backpressure in stress checks.
The server caps active streams at four and exits on SIGINT/SIGTERM.

The embedded benchmark page exposes `window.bench.connect('sse'|'websocket')`,
`calibrate()`, `reset()`, `freeze(true|false)`, and `result()` for browser automation.
Receipt and JSON-decoding timings are distinct from DOM-update timings; none are
presented as physical screen paint. One pending frame is rendered at the next
animation frame. Sample storage is capped, with overflow reported as invalid data.
Calibration maps the browser monotonic clock to the server epoch using the fastest
of 12 local round trips; report at least half its RTT as timing uncertainty.

Browser checks need Playwright 1.62.1 and its Chromium. Either install these as
development tools or point `PLAYWRIGHT_MODULE_DIR` at an existing `playwright`
package directory, then run `node --test experiments/web-transport/browser.test.mjs`.

For a fresh development setup, run `npm ci` and `npx playwright install chromium`
inside this directory. No JavaScript tooling is needed by the released CLI.

Run a paired browser campaign from the repository root:

```sh
node experiments/web-transport/benchmark.mjs --hz 10 --clients 0,1,4 \
  --profiles representative,large-counters --pairs 5 --seconds 10 --warmup 1
```

Transport order alternates within successive pairs. Each run uses a fresh server
and browser context, with one shared Chromium process across the campaign. The
four-page case keeps all documents visible in headless Chromium with background
throttling disabled; it is a foreground stress case, not four ordinary focused
desktop tabs. Warmup and clock calibration precede measured intervals.

The full JSON contains raw per-message browser samples and per-run resources.
A separate `-summary.json` reports distributions of client/run percentiles; those
are not pooled-message percentiles. Clock uncertainty and offset drift remain
visible, and negative calibrated timings are not clamped. DOM timings end after
DOM/SVG writes, not physical paint. Received/rendered totals distinguish browser
render coalescing from publication gaps. The publication timestamp is taken before
JSON encoding, after cloning/converting metrics. `ps` samples process-tree CPU/RSS about once
per second; CPU totals have OS rounding and exclude exited children whose final
accounting is unavailable. Browser heap snapshots include benchmark sample storage.
The protocol-only replay campaign does not establish ingestion overhead, input
freshness, hidden-tab behavior, or a long-run memory plateau.

`node experiments/web-transport/pipeline.mjs` separately measures 25 marker trials
per transport with real stdin/file following, idle and nominally paced 20K
records/second. All injected records must be accounted for exactly. Timing uses
one host monotonic clock from producer write through Playwright's observation of
the DOM; automation round-trip cost is reported, not subtracted. The benchmark
samples `Running::snapshot()` at 10 Hz, adding a possible sampling interval on top
of engine publication; the planned notification subscription is not implemented
by this experiment. Paced input is a correctness/freshness workload and does not
measure maximum ingestion capacity or establish the 5% overhead target.

Frames use actual engine statistics with 60 completed history buckets. The
`large-counters` profile deliberately widens cumulative counters/history to u64
limits; it is a payload-width stress case, not a consistent ingestion ledger.
Counter strings avoid losing integer precision in the browser. Both transports
share the same JSON bytes, serialized once per publication. This experiment's
envelope is separate from the future production web contract and CLI schema 2.

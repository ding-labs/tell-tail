# Direct snapshot notifications

Implemented October 20, 2026. A web publisher can now react to each native
publication without adding a polling interval. The experimental server uses this
path for real stdin and followed files; fixture replay keeps its synthetic timer.
The [production web server](WEB-SERVER.md) is implemented; the dashboard remains
the next increment.

## API and ownership

`Running::snapshot_reader()` returns an owned, read-only `SnapshotReader` that can
outlive ingestion. `latest()` clones a `PublishedSnapshot` containing the current
`generation` and `snapshot`. Generation zero is initial state; each native
publication advances it. This metadata does not change CLI JSON schema 2.

Cargo feature `snapshot-notifications` enables the asynchronous subscription and
is also included by default through `web`:

```rust
let mut publications = running.snapshot_reader().subscribe()?;
while let Some(publication) = publications.changed().await {
    // Encode publication.snapshot and fan out the owned frame here.
}
running.join()?;
```

- The first call returns current state immediately, including a final snapshot
  when ingestion has already completed. Later calls wait for a new generation.
- Intermediate publications coalesce. A slow receiver obtains the latest complete
  snapshot; there is no queue to grow or replay. Alert evaluation still sees the
  existing native publication path.
- One asynchronous publisher can subscribe at a time. A second subscription
  returns `io::ErrorKind::AlreadyExists`. Fan out serialized frames from the one
  publisher to browser clients. Dropping a subscription permits replacement,
  even when a previous notification is in flight.
- Cancelling a pending `changed()` wait does not consume an unseen generation.
  Dropping a reader/subscription does not stop ingestion.
- The worker closes publication on normal exit and unwinding. An unseen last
  publication is delivered before `None`. A panic does not invent a final state;
  use `Running::join()` to propagate the worker result.

The existing snapshot mutex protects generation, state, and registration together.
Publication releases it before calling `Notify::notify_one`. Notification permits
coalesce; generation checks handle publication before/during wait registration and
discard stale permits. Encoding and network I/O receive owned snapshots with no
ingestion lock retained. The worker still publishes approximately every 100 ms,
plus immediate final state; this change adds no per-record work.

The standalone `--no-default-features --features snapshot-notifications` build
enables only Tokio's `sync` primitives. The subscription starts no runtime; the
caller supplies the executor. Default builds now include web support, whose
runtime starts only in web mode. `--no-default-features` excludes Tokio from the
active normal dependency graph. The asynchronous subscription adds one weak
registration and one notifier for the active publisher, independent of browser-client count.

## Local freshness validation

The [measurement record](validation/snapshot-notifications-2026-10-20.json)
contains the tested source revision, binary hash, host/toolchain, audit identity,
and all eight input scenarios. There are 25 host-monotonic producer-write-to-DOM
trials per scenario, with exact injected counts and automation overhead included.

The table compares p95 milliseconds with the [earlier polling run](WEB-TRANSPORT.md).
These are separate local runs on the same Apple M3/Chromium setup, not randomized
paired trials or physical screen-paint measurements.

| Source / load | SSE polling → notifications | WebSocket polling → notifications |
| --- | ---: | ---: |
| Stdin / idle | 218.7 → 136.1 | 191.1 → 135.9 |
| Stdin / paced 20K records/s | 205.8 → 107.5 | 183.7 → 103.1 |
| File / idle | 183.6 → 109.4 | 188.6 → 108.0 |
| File / paced 20K records/s | 190.3 → 106.6 | 189.1 → 105.6 |

All final counts matched. Maximum per-scenario p99 was 150.8 ms, and maximum
observed real-input server shutdown was 78.9 ms. This removes the web publisher's
extra timer; input readiness, native publication cadence, scheduling, and browser
rendering still contribute latency. Maximum ingestion overhead and long-duration
resource gates have not been established by this paced workload.

Verification passed: 55 default Rust tests, 61 with notifications enabled, two
experiment Rust tests, and 11 Node/network/browser checks including native hidden-tab
resume. Clippy passes with warnings denied in default and notification modes.
Tests cover slow/cancelled receivers, replacement, publication/closure races,
idle-window expiry, EOF without the replay timer, a quiet retained final stream,
and exact live-input totals with an unread web client. CI is configured for both feature modes
on macOS and Linux; the new CI configuration has not been executed remotely here.

Reproduce after building the release experiment:

```sh
cargo test --locked
cargo test --features snapshot-notifications --locked
node experiments/web-transport/pipeline.mjs --samples 25 \
  --output experiments/web-transport/results/pipeline-notifications.json
```

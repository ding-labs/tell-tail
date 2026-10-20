# Local web UI plan

Status: production loopback server and lifecycle implemented, October 20, 2026. The
[local transport measurements](WEB-TRANSPORT.md) support SSE, and the
[subscription and freshness results](SNAPSHOTS.md) cover direct publication.
The [web frame contract](WEB-FRAMES.md) provides bounded version 1 encoding.
The [production server](WEB-SERVER.md) supplies bounded HTTP/SSE and CLI lifecycle.
The dashboard remains planned; this does not change the
0.2 release gates.
The first UI covers live aggregate metrics, as agreed. Raw log viewing and search
are outside this milestone.

## Recommendation

Serve an embedded dashboard from the Tell-Tail binary on loopback, and push full
metrics snapshots over Server-Sent Events (SSE). Start at the engine's existing
approximately 100 ms publication cadence. Use one serializer and bounded,
latest-value delivery to every browser.

The completed comparison uses the same payload and browser renderer for SSE and
WebSocket. SSE is the selected transport for this one-way workload; the local
measurements do not establish that it is universally the fastest protocol.

HTTP does not imply polling: SSE keeps an HTTP response open and delivers events
incrementally. WebSocket provides two-way messages after its handshake. gRPC
normally runs over HTTP/2; browser clients need a browser-compatible protocol
such as gRPC-Web. These are different communication models, not a simple ranking
of HTTP versus faster alternatives.

## What already exists

- [`runtime.rs`](../src/runtime.rs) ingests on a dedicated thread and replaces
  one versioned snapshot approximately every 100 ms, plus a final snapshot.
  Publication occurs between input batches, so this is not a hard deadline.
- [`snapshots.rs`](../src/snapshots.rs) supplies read-only handles and an optional
  single-publisher subscription with coalescing notifications and closure handling.
- [`stats.rs`](../src/stats.rs) supplies rates, severity counts, rolling and
  cumulative totals, source state, diagnostics, and up to 60 completed seconds
  of throughput history. Rolling statistics use 100 ms buckets.
- [`tui.rs`](../src/tui.rs) supplies the initial product model: throughput,
  warning/error ratios, severity distribution, history, and display freeze.
- [`main.rs`](../src/main.rs) selects output and owns process shutdown. Existing
  text/JSON output defaults to a one-second interval, independently of the
  engine's publication cadence.

Integrate with the in-process snapshot handoff. Launching another Tell-Tail
process and parsing its NDJSON would add lifecycle and output scheduling work.

## Transport decision

| Option | Benefits for this workload | Costs and limits | Decision |
| --- | --- | --- | --- |
| HTTP polling | Simple request/response, useful for inspecting current state | Repeated requests and an extra polling interval; faster polling adds work | Provide a snapshot endpoint for diagnostics, not the live update path |
| SSE / EventSource | Native browser push, automatic reconnect, event IDs; one persistent response | UTF-8 text; HTTP/1.1 connection limits matter across tabs; application must bound buffering | Preferred initial transport |
| WebSocket | Full duplex, binary payloads, compact message framing | Reconnect/resume protocol is application work; ordinary browser WebSocket has no automatic receive backpressure | Compare experimentally; use if measurements or later interaction needs justify it |
| gRPC-Web | Typed RPC contract and protobuf tooling | Browser-compatible server/adapter and generated client; official client supports unary and server streaming, with streaming in text mode | Excess machinery for one local snapshot stream |

SSE behavior is specified in the [HTML standard][sse-spec]. The
[browser guide][sse-guide] documents the HTTP/1.1 connection limit.
[RFC 6455][websocket-spec] defines WebSocket framing and two-way communication;
the [browser WebSocket API][websocket-api] documents its backpressure limitation.
The [official gRPC-Web client][grpc-web] documents its supported RPC modes.
An in-process gRPC-Web adapter can avoid an external proxy, but still adds another
protocol and code-generation contract.

At an illustrative 4 KiB per snapshot and 10 updates/second, payload traffic is
about 40 KiB/second per tab. This is an estimate to verify with real snapshots.
Both SSE and WebSocket avoid a request round trip per update; at this scale,
publication timing, buffering, and browser rendering are likely to matter more
than framing bytes. Binary encoding and compression require evidence of a
bottleneck before adding them.

## User experience and CLI

Available server commands:

```sh
tell-tail app.log --output web
my-app 2>&1 | tell-tail --output web
tell-tail app.log --output web --web-port 8765
```

The interactive dashboard and `--open` remain planned in increment 5.

- Bind `127.0.0.1` with an OS-selected port by default. Print the URL using the
  existing bounded diagnostic path. `--open` optionally launches the system
  browser without a shell; opener failure leaves a usable printed URL.
- `--web-port` accepts 0 through 65535 and requires web output. An occupied
  requested port fails clearly; bind before starting ingestion.
- Web output serves one file follower or stdin and emits no periodic stdout.
  Existing `--interval-ms` retains its current output-specific meaning; web
  publication starts at 100 ms without a new rate setting.
- Reject `--once --output web` in this first milestone: `--once` promises exit
  at EOF. A separate finite-report experience can be designed later.
- Normal stdin EOF freezes the final metrics and keeps the server available
  until Ctrl+C. State this explicitly in help and the completion banner.
  Browser tab closure does not stop ingestion.
- SIGINT/SIGTERM stop ingestion and the server within the existing one-second
  shutdown target. Attempt final delivery for at most 250 ms, without requiring
  a browser acknowledgment. Preserve signal exit codes; runtime failure returns
  1. Fatal ingestion/server errors attempt final failure delivery and exit.
- A completed page retains its final values. A disconnected page retains its
  last values with a visible connection warning; it must not claim completion
  unless it received a final state.

The initial screen contains source/state, elapsed time, entries/second, MiB/second,
warning and error/fatal ratios, a 60-second throughput chart, severity counts,
and expandable ingestion diagnostics. Label rolling values separately from
session totals, and show the observed window duration during warmup.

Freeze affects the displayed snapshot only; the connection can retain one newer
snapshot for immediate resume. Connection/failure status continues updating while
frozen. Keyboard access, responsive layout, readable contrast, and status text
independent of color are part of the first usable dashboard.

## Implementation architecture

```text
file / stdin
    -> existing ingestion thread and Stats
    -> latest Snapshot + publication generation
    -> coalescing notification to one web publisher
    -> serialize once into a shared immutable frame
    -> latest-frame channel -> bounded SSE response per tab
    -> one pending browser snapshot -> scheduled DOM/chart update
```

Use Axum for HTTP routing/SSE and a Tokio runtime active only in web mode.
[Axum provides SSE responses][axum-sse]; Tokio's [watch channel][tokio-watch]
retains the latest value and suits frame distribution. Prefer a single runtime
thread initially. Keep the parser and ingestion loop synchronous.

The implemented `Running::snapshot_reader()` exposes a read-only handle and
internal publication generation. With Cargo feature `snapshot-notifications`,
`subscribe()?.changed().await` returns initial state immediately and then waits
for new generations. [`Notify::notify_one`][tokio-notify] coalesces wakeups, with
one active publisher registration. The owned result carries no lock into
serialization. Startup, cancellation, final-publication races, and idle expiry
have regression coverage; see the [subscription contract](SNAPSHOTS.md).

Share the resulting bytes through a `watch` channel. Clone the frame handle and
release every channel borrow before I/O or awaiting. Ingestion must never wait
for serialization, a client socket, or a per-client queue. Alerts continue to
observe snapshots through the existing path.

The independent notification feature enables only Tokio's `sync` primitives.
The implemented, default-enabled Cargo `web` feature adds pinned Axum/Hyper and
Tokio I/O, time, and single-thread runtime support. A CLI-only build remains
available with `--no-default-features`. Versions build with Rust 1.88.0, and the
[dependency review](DEPENDENCIES.md) records licenses and audit results.

Use embedded HTML, CSS, and small JavaScript modules with JSDoc types and an SVG
chart. Sixty points and a few counters do not need a charting framework. Keep
assets in `web/`, embed them at compile time, and add them to Cargo's package
include list. A release/source installation must work offline without Node,
a CDN, or a separate frontend server. Browser test tooling is development-only.

## Stream and data contract

- `GET /` and fixed asset routes serve the embedded application.
- `GET /api/snapshot` returns the current web frame for inspection/recovery.
- `GET /api/events` sends the current frame immediately, then newer frames.
  Send `Content-Type: text/event-stream` and `Cache-Control: no-store`;
  disable compression/batching on the stream and verify incremental delivery.
  Send a 500 ms reconnect hint; measure actual browser retry behavior rather
  than treating that hint as a guaranteed deadline.
- A `snapshot` SSE event carries a complete frame. Its ID combines the session
  ID and publication generation. Reconnection always sends current state;
  `Last-Event-ID` does not promise replay. Sequence gaps are expected when a
  consumer falls behind. A new session resets frontend state.
- A final frame uses the same event shape and contains the final engine state.
  On receipt, the browser closes EventSource. A newly opened page after normal
  EOF gets that final frame immediately. Heartbeat comments every 15 seconds
  can maintain an otherwise idle stream but do not make old metrics fresh.

The implemented [web schema version 1](WEB-FRAMES.md) envelope contains `web_schema_version`,
`sequence`, and `metrics`. Reuse the existing snapshot's meanings and field
names inside `metrics`, omitting its CLI `schema_version` field. Encode all u64
counters, byte counts, history counts, and sequence numbers as decimal strings;
keep rates, durations, bounded configuration values, and nullable ratios numeric.
This avoids JavaScript integer precision loss without changing CLI JSON schema 2.
Use BigInt for exact count formatting; any chart conversion to Number is an
explicit approximation. Cover values beyond `2^53` and empty/null denominators.

The existing history is completed per-second record counts, not a history of
rolling rates. Use its documented time alignment, replace it from each complete
frame, and do not append the same second on every 100 ms update. Do not invent
historical warning/error series that the engine does not retain.

## Bounds, connection handling, and local access

Start with four simultaneous SSE clients and 16 total HTTP connections, with
admission before spawning connection tasks. Give request headers a two-second
deadline and an 8 KiB cap. Data routes accept GET only, and reject request bodies.
Cap serialized web frames at 64 KiB; an oversize frame produces a bounded error
and fails web output rather than silently displaying old data indefinitely.

Each client holds at most one current frame plus the shared latest frame.
Bound HTTP-library write buffers, and close a connection whose pending write
makes no progress for two seconds. Check actual socket progress: a body stream
being polled is not proof that a browser received bytes. Cancellation interrupts
these waits. Test a genuinely undrained socket and partially sent events.
Dropping intermediate frames never removes counts from cumulative totals.

Native EventSource and kernel buffers can still contain stale frames. Keep one
pending frame in the renderer, coalesce updates into `requestAnimationFrame`,
and close the stream while the page is hidden. On visibility return, reconnect
for current state. Show stale/reconnecting status after one second without a
new live snapshot; evaluate freshness on visibility changes as timers can pause.
Browser freeze retains only one latest frame, never an accumulating history.

HTTP/1.1 is sufficient for the supported tab count; more than four streams gets
a clear capacity state. Check capacity via the snapshot endpoint when EventSource
cannot expose the failure status, and avoid an uncontrolled retry loop.
More tabs or remote deployment would require revisiting connection sharing or
HTTP/2, not silently increasing limits.

Keep the listener loopback-only, validate the exact Host including port, reject
foreign Origin headers, and emit no cross-origin access grants. Serve only fixed
embedded routes, escape source text through DOM text APIs, and set a restrictive
content policy including no framing. This is a local, same-machine tool; remote
access, authentication, filesystem browsing, and mutable configuration APIs are
separate product work.

## Performance experiment and acceptance

Before building the full UI, implement a development-only comparison of SSE and
WebSocket JSON from the same publisher into the same minimal renderer. Use
actual captured snapshot sizes, a full 60-point history, and worst-case counters.
Test 10 Hz as the product workload; 50/100 Hz synthetic publication is stress
testing and does not imply the current engine produces data that quickly.

Measure zero, one, and four clients; active, frozen, hidden/resumed, and stalled
consumers; file and stdin input; idle and sustained ingestion. Pair and alternate
transport runs on the same named host, with warmup and at least five pairs.
Record browser/OS/build versions, payload sizes, p50/p95/p99, CPU, process-tree
RSS, browser heap, socket buffering, and ingest throughput.

Separate the latency stages: input-to-snapshot, publication-to-event receipt,
JSON decoding, and DOM update. For end-to-end trials, inject a known count change
and observe that exact count in the browser using one host monotonic clock.
Browser/host cross-clock timings require calibration; do not subtract unrelated
monotonic clocks or present a server write as a browser paint. Report automation
overhead, and visually inspect rendering in addition to DOM timing.

Proposed gates below are targets, not existing results:

| Property | Acceptance target |
| --- | --- |
| Responsiveness | Foreground write-to-DOM p95 <= 250 ms and p99 <= 500 ms on named, unsaturated reference hosts |
| Ingestion overhead | Under 5% median loss versus the same build with web mode disabled, with one active browser; report variation and four-client results separately |
| Fresh reconnect | Current state within one second of a foreground reconnect request to a reachable local server; report total outage/retry time separately |
| Bounded resources | No growing snapshot queue; Tell-Tail RSS and browser heap plateau during a one-hour run and repeated connection churn |
| Isolation | Stalled client leaves ingestion, other clients, alert evaluation, and shutdown responsive |
| Shutdown | Under one second for supported input/socket cases, including stalled clients |

Use SSE if it meets the gates and WebSocket differences are within run variation
or immaterial to visible responsiveness. Switch to WebSocket if repeatable
measurements show a material benefit that warrants its additional protocol code,
or if a future feature needs sustained two-way traffic. Investigate publication
and rendering delays before attributing an end-to-end miss to transport.
Do not ship both live transports without a demonstrated need.

## Delivery increments

Verify and commit each coherent increment before starting the next independent
change. Split the following milestones into smaller commits where appropriate.

1. **Transport experiment — complete.** The isolated harness compares SSE and
   WebSocket on Rust 1.88, with results in `docs/WEB-TRANSPORT.md`. Experiment
   routes remain outside release builds.
2. **Snapshot subscription — complete.** Read-only handles, generations, and
   coalescing wakeups are implemented. Idle expiry, startup/final races, slow and
   cancelled consumers, exact counts, alerts, and compatibility are covered.
3. **Web frame contract — complete.** A typed encoder preserves exact u64
   counters, nulls, states, and completed-second history in web schema 1. Encoding
   enforces the 64 KiB JSON limit and rejects invalid data. The experiment uses
   production frames with separate benchmark metadata; see `docs/WEB-FRAMES.md`.
4. **Loopback server and lifecycle — complete.** CLI web output serves current
   JSON and SSE with admission/header/buffer limits and bounded teardown. Tests
   cover reconnect, EOF, signals, input/encoder failure, invalid hosts/origins,
   occupied ports, malformed/slow requests, and actual undrained socket writes.
   See `docs/WEB-SERVER.md`; the root page is informational until increment 5.
5. **Usable embedded dashboard.** Build against real frames, then add responsive
   layout, freeze/resume, freshness/final/capacity states, and browser opening.
   Verify keyboard use, null/large values, and safe source rendering. Use browser
   tests for actual interactions and reconnect/visibility transitions.
6. **Validation and packaging.** Run the performance gates and browser soak;
   check Chrome, Firefox, and Safari where available, macOS/Linux lifecycle,
   default/CLI-only Cargo builds, and an extracted source package offline.
   Update README, operations, dependency notices, and validation evidence.

Done means a single installed binary serves a usable live dashboard with measured
freshness, bounded resource use, reliable disconnect/shutdown behavior, and
unchanged existing output contracts. Raw logs, multiple sources, persistent
history, remote access, and an HTTP Prometheus endpoint remain later increments.

[sse-spec]: https://html.spec.whatwg.org/multipage/server-sent-events.html
[sse-guide]: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events
[websocket-spec]: https://www.rfc-editor.org/rfc/rfc6455.html
[websocket-api]: https://developer.mozilla.org/en-US/docs/Web/API/WebSocket
[grpc-web]: https://github.com/grpc/grpc-web
[axum-sse]: https://docs.rs/axum/latest/axum/response/sse/index.html
[tokio-watch]: https://docs.rs/tokio/latest/tokio/sync/watch/index.html
[tokio-notify]: https://docs.rs/tokio/latest/tokio/sync/struct.Notify.html

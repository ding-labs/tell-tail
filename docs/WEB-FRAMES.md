# Web frame contract, version 1

Implemented October 20, 2026 in [`web_frame.rs`](../src/web_frame.rs). The library
encodes production frames without HTTP, Tokio, or new dependencies. The
[transport experiment](../experiments/web-transport/README.md) now uses this
encoder. The [production loopback server and CLI web mode](WEB-SERVER.md) are
implemented, along with the [embedded dashboard](WEB-DASHBOARD.md).

## API and ownership

After obtaining an owned publication through the [snapshot API](SNAPSHOTS.md):

```rust
let frame = std::sync::Arc::new(tell_tail::web_frame::WebFrame::encode(
    publication.generation,
    &publication.snapshot,
)?);
```

`WebFrame::encode(sequence, &snapshot)` returns `Result<WebFrame, EncodeError>`.
The immutable frame exposes `sequence()`, `json()`, and `event_id()`. Encode once
outside the ingestion lock, share the frame with `Arc`, and retain only the latest
frame for fanout. The encoder borrows the snapshot without cloning it or building
an intermediate JSON object. Its output owns its strings and outlives the snapshot.

## JSON shape and types

Every payload has exactly these envelope fields:

```json
{"web_schema_version":1,"sequence":"0","metrics":{}}
```

The example abbreviates `metrics`; see the complete
[empty final fixture](../tests/fixtures/output/empty-web-v1.json).
`web_schema_version` is the numeric web protocol version, independent of CLI JSON
schema 2. `metrics` preserves the snapshot's field names and meanings and omits
its CLI `schema_version`. Benchmark timing and padding are absent.

| Field | JSON representation |
| --- | --- |
| `sequence` | Decimal u64 string; native publication generation, starting at zero |
| `metrics.kind`, `session_id`, `source` | Strings |
| `metrics.state` | `reading`, `following`, `waiting`, `complete`, `stopped`, or `failed` |
| `observed_at_unix_seconds`, `elapsed_seconds` | Finite numbers |
| `totals.records`, `totals.levels.*`, `totals.parse_failures`, `totals.oversized_records` | Decimal u64 strings |
| `bytes_read`, `pending_bytes`, `skipped_bytes`, `discarded_partial_records` | Decimal u64 strings |
| `backlog_bytes` | Decimal u64 string, or `null` when unavailable |
| `warning_ratio`, `error_ratio` | Finite numbers, or `null` for an empty denominator |
| `window.configured_seconds` | Number; engine configuration is 1 through 3600 |
| `window.resolution_seconds`, `window.observed_seconds` | Finite numbers |
| `window.counts.*` | Same counter types as `totals`, including all seven levels |
| `window.bytes_read` | Decimal u64 string |
| `window.records_per_second`, `window.bytes_per_second` | Finite numbers |
| `window.warning_ratio`, `window.error_ratio` | Finite numbers, or `null` for an empty denominator |
| `history` | Array of at most 60 decimal u64 strings, oldest first |

All fields below the envelope in this table are inside `metrics`. Decimal strings
are canonical unsigned base-10 values, from `"0"` through
`"18446744073709551615"`, without signs or leading zeroes. Use browser `BigInt`
for exact counts and sequence comparisons. Converting to `Number` for a chart is
an explicit approximation. Zero is a value; it must not replace `null` ratios or
an unavailable backlog. The encoder preserves snapshot values rather than
recalculating counters or ratios.

Rolling windows retain the whole oldest 100 ms bucket. `window.observed_seconds`
is nonnegative and strictly less than `configured_seconds + resolution_seconds`
(with `resolution_seconds: 0.1`); it can exceed the configured duration by less
than 100 ms after warmup. Rates use this observed duration.

History contains counts for completed process-elapsed seconds, not rolling rates
or wall-clock timestamps. With `end = floor(elapsed_seconds)` and `n = history.length`,
point `i` covers `[end - n + i, end - n + i + 1)`. Startup history is empty until
the first second completes. Idle seconds contain zero. Replace history from each
frame; appending on each approximately 100 ms update would repeat seconds. Warning
and error histories are not retained by the engine and are not present here.

## Sessions, sequence numbers, and final state

An SSE `snapshot` event carries `json()` as its data. `event_id()` is
`<session_id>:<sequence>` and is safe on one `id:` line. JSON is compact UTF-8 with
CR, LF, and NUL escaped inside strings; source text cannot add SSE fields.
Serve JSON as data and render source names through DOM text APIs.

Within one session, accept the first frame even at sequence zero, then ignore
duplicate or older sequences. Gaps are expected with coalescing subscribers and
do not subtract from cumulative totals. On a new session ID, reset the sequence
comparison and pending history. Reconnecting obtains current state; event IDs do
not promise retained event replay. Clients should reject unsupported web schema
versions rather than interpreting their fields as version 1.

Live states use `kind: "snapshot"`. Complete, stopped, and failed states use
`kind: "final"` with the same fields and types. A final snapshot can retain
nonempty rolling metrics and history; finality does not clear them. A transport
disconnect alone must not be interpreted as a final state.

## Bounds and errors

`MAX_FRAME_BYTES` is 65,536 **serialized UTF-8 JSON bytes**, excluding SSE framing
and event IDs. JSON escaping counts toward the cap. Exactly 65,536 bytes is valid;
one additional byte fails. The encoder writes into a buffer with a fixed 64 KiB
capacity and stops at the cap, without first allocating a larger serialized value.
The event ID is bounded by the session ID already admitted inside that JSON.

Encoding fails with a bounded diagnostic for an oversized frame, more than 60
history points, an empty session ID or one containing CR/LF/NUL, or a non-finite
metric. Non-finite numbers must not silently turn into JSON `null`. Error messages
do not echo input strings. Treat an error as fatal to the web publisher/output;
continuing to serve an older frame indefinitely would hide the failure. The
experiment propagates these errors through its publisher and server result.

## Verification

[`tests/web_frame.rs`](../tests/web_frame.rs) covers the reviewed empty fixture,
all states and nullable values, every u64 field through `u64::MAX`, history across
startup/second boundaries/ring reuse, UTF-8 and escaping at the exact size limit,
and invalid data. The CLI schema 2 compatibility fixture remains unchanged.

At the frame-contract milestone, before default web support, verification passed
on macOS / Rust 1.88.0: 61 default Rust tests, 67 with
snapshot notifications, three experiment Rust tests, and 12 Node/network/browser
checks including native hidden-tab resume. Clippy passes with warnings denied
in both root feature modes and the experiment. Existing CI runs root tests in both
modes on Linux and macOS; remote CI has not been run in this session.

The [validation record](validation/web-frames-2026-10-20.json) includes four short
replay smoke cases and eight five-marker real-input cases with exact final counts.
These establish integration correctness, not a new transport ranking, ingestion
overhead bound, or long-duration resource result. Historical benchmark schema 1
measurements remain unchanged; the experiment now wraps web frames in benchmark
schema 2 and reports both versions.

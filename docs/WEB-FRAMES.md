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

Every payload has these required envelope fields:

```json
{"web_schema_version":1,"sequence":"0","metrics":{}}
```

The example abbreviates `metrics`; see the complete
[empty final fixture](../tests/fixtures/output/empty-web-v1.json).
`web_schema_version` is the numeric web protocol version, independent of CLI JSON
schema 2. `metrics` preserves the snapshot's field names and meanings and omits
its CLI `schema_version`. Benchmark timing and padding are absent.

An optional top-level `recent_errors` array adds dashboard previews to version 1.
It is omitted when empty; older clients can ignore it. Each full frame replaces
the array with at most 20 entries, newest first. Entries have `record` (an exact
decimal u64 string), `elapsed_seconds` (nonnegative processing time), `level`
(`error` or `fatal`), `preview` (at most 512 UTF-8 bytes of the input record), and
`truncated` (boolean). Control characters are replaced with spaces. Capture is
enabled only for terminal and web dashboards, remains in memory, and is omitted
from CLI JSON and webhook payloads. This array is a recent sample, with no replay
or persistence guarantee. It shares the existing 64 KiB frame limit.

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
frame; appending on each approximately 100 ms update would repeat seconds.
The additive severity history uses these same intervals, as described below.

## Coverage, severity history, and activity (unreleased)

These additions on current main are not in the published 0.2.0 packages. They
retain web schema version 1: existing `metrics` fields keep their meanings, and
clients may ignore unknown envelope fields. The embedded browser accepts older
version 1 frames that omit the additions. CLI JSON remains schema 2.

| Field | Current encoder | Meaning |
| --- | --- | --- |
| `severity_history` | Omitted when empty; otherwise an array of at most 60 objects | Completed seconds aligned one-for-one with `metrics.history`, oldest first |
| `severity_history[i].warn` | Decimal u64 string | Warning records in that second |
| `severity_history[i].error` | Decimal u64 string | Error **plus fatal** records in that second |
| `activity` | Always an object | Ages measured at this snapshot |
| `activity.last_record_age_seconds` | Finite nonnegative number or `null` | Seconds since the last counted complete record |
| `activity.last_error_age_seconds` | Finite nonnegative number or `null` | Seconds since the last counted error/fatal record |

For example, this **excerpt** describes two completed seconds at elapsed time
2.4 seconds. Other required metrics and envelope fields are omitted here; it is
not a standalone frame:

```json
{
  "metrics": {
    "elapsed_seconds": 2.4,
    "history": ["4", "2"]
  },
  "severity_history": [
    {"warn": "1", "error": "1"},
    {"warn": "0", "error": "2"}
  ],
  "activity": {
    "last_record_age_seconds": 0.1,
    "last_error_age_seconds": 0.4
  }
}
```

The first point covers `[0, 1)` and the second `[1, 2)`. The in-progress interval
`[2, 2.4]` is absent from history even though its records already contribute to
session and rolling metrics. Histories retain idle zeroes, expire after 60
completed seconds, and are independent of `--window`. Replace both arrays on
each accepted frame. Do not append, carry old severity points forward when the
field is absent, or infer zero errors from an older frame without that field.
Before the first second completes, the current encoder emits `metrics.history:
[]` and omits `severity_history`.

Activity uses monotonic **processing time**, not log event timestamps or browser
wall time. Unknown and oversized completed records reset the record age; skipped
Apple metadata and partial bytes do not. A final nonempty unterminated line at
finite EOF counts. Ages survive rolling-window expiry and are no greater than
`metrics.elapsed_seconds`. `null` means no matching record observed in this
session. Missing `activity` in a legacy frame means unavailable, which is a
different state. A zero age is a valid observation.

Render these ages as snapshot values. Frozen, disconnected, or final displays
must not advance them with a local clock and imply fresh measurements. A client
may separately show how old its last received snapshot is. Restarting the
monitor creates a new session and resets observations.

### Deriving coverage and interval comparisons

Coverage has no new serialized field. Derive it from `metrics.window.counts`:

```javascript
const counts = frame.metrics.window.counts;
const records = BigInt(counts.records);
const recognized = records - BigInt(counts.levels.unknown);
// Exact numerator and denominator; do not round until formatting for display.
const coverage = records === 0n ? null : { recognized, records };
const malformed = BigInt(counts.parse_failures);
const oversized = BigInt(counts.oversized_records);
const otherUnknown = BigInt(counts.levels.unknown) - malformed - oversized;
```

This measures recognized severity, not application health or a probabilistic
confidence estimate. Unknowns remain in the denominator of warning/error ratios.
Malformed and oversized records are disjoint subsets of unknowns. An empty
window has unavailable coverage, not 0% or 100%. The
[monitoring guide](MONITORING.md#parsing-coverage-what-the-percentage-means) explains supported
parsing conventions and a worked example.

The browser's comparison requires 60 points in both histories. Sum error counts
and total counts separately over indices `[30, 60)` and `[0, 30)`, then divide
each error sum by its corresponding total sum. Do not average per-second
percentages. A zero total has no ratio. At elapsed 75.6 seconds, these intervals
are `[45, 75)` and `[15, 45)`; neither includes the current partial second.
This is a display comparison, not an anomaly detector or an alert rule.

### Output compatibility

| Output | Coverage | Warning/error history | Activity ages |
| --- | --- | --- | --- |
| Terminal dashboard | Shown for the rolling window | Sparklines | Shown at the displayed snapshot |
| Web dashboard / web schema 1 | Shown / derived from existing counters | Additive `severity_history` field | Additive `activity` field |
| CLI JSON schema 2 | Derivable from existing `window.counts` | Not serialized | Not serialized |
| Text, Prometheus, webhook payloads | No new coverage field | No new history field | No new activity field |

The CLI's existing total-record `history` is unchanged. Raw error previews use
the separate bounded `recent_errors` field described above. None of these
additions enable retention, log forwarding, or new webhook triggers.

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

Encoding fails with a bounded diagnostic for an oversized frame, an empty
session ID or one containing CR/LF/NUL, or a non-finite metric. `InvalidHistory`
rejects more than 60 history points, unequal total/severity history lengths, or
a point whose warning-plus-error count overflows u64 or exceeds the total count.
`InvalidActivity` rejects any supplied age that is non-finite, negative, or
greater than session elapsed time. The browser validates these same history and
age constraints when the additive fields are supplied. Both arrays, activity,
and previews share the existing 64 KiB limit; there is no extra per-feature budget. Non-finite numbers must not silently turn into JSON `null`. Error messages
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

# Tell-Tail MVP plan

Tell-Tail measures log throughput and severity while discarding log contents after
processing. Its state consists of counters, reusable buffers, and bounded time
buckets. Loki and similar systems remain responsible for log storage and search.

## Agreed scope

- One regular file or stdin per process. Multiple sources follow later.
- Local development on macOS and Linux, plus long-running Linux pipelines.
- A terminal dashboard and optional plain text, NDJSON, or Prometheus output.
- Portable parsing and statistics, with platform-specific I/O isolated for later
  Windows support.
- Session totals and a configurable rolling window, default 60 seconds and capped
  at one hour. No raw log retention or application database.

## Command contract

```sh
tell-tail app.log                       # follow new records
tell-tail app.log --from start          # replay, then follow
tell-tail app.log --once                # scan from the beginning, then exit
my-app 2>&1 | tell-tail                 # consume stdin until EOF
tell-tail app.log --format json --level-field severity
tell-tail app.log --window 300          # seconds
tell-tail app.log --output json > stats.ndjson
tell-tail app.log --once --output prometheus
tell-tail app.log --output prometheus --metrics-file tell-tail.prom
```

Automatic output selects the terminal dashboard when a usable controlling
terminal and terminal stdout are present, and plain text otherwise. Diagnostics
go to stderr; machine reports go to stdout or the explicitly selected metrics
file. Piped log input and dashboard keyboard input must use separate descriptors.
No input plus interactive stdin prints usage. Contradictory options are errors.

## Counting and parsing

- One physical line is one record, including blank lines. Multiline stack traces
  count as multiple records. A trailing newline creates no extra record.
- Count a final nonempty unterminated record at finite EOF. While following,
  retain a bounded partial line; report partial bytes on interruption.
- Categories: trace, debug, info, warn, error, fatal, unknown. Normalize case and
  documented aliases. Unknown records stay in the denominator.
- Warning ratio is warn / total. Error ratio is (error + fatal) / total. Empty
  windows have an undefined ratio: a dash in text and null in JSON.
- NDJSON uses a configurable top-level field, default `level`. Validate the
  complete JSON object, including fields after the severity. Avoid building an
  object tree. Malformed JSON is unknown and increments a parse-failure counter.
- Plain text accepts a severity at the start, optionally bracketed, optionally
  preceded by an ISO timestamp. Message text never determines severity.
- Auto format selects JSON for a line whose first non-whitespace byte is `{`;
  otherwise it uses the text grammar. Malformed JSON never falls back to text.
- A default 1 MiB record limit bounds partial-line memory. Drain oversized records
  to their terminator, count once as unknown, and increment an oversized counter.
  Count all bytes read, including delimiters and discarded oversized bytes.

## Engine architecture

One ingestion worker owns input, framing, classification, counters, and a ring of
one-second buckets. The presentation loop consumes compact snapshots through a
bounded latest-value handoff. It may skip intermediate snapshots without losing
cumulative counts. No raw-record queue, per-record locking, or async runtime is
needed. Buffers are reused, and complete records are parsed directly from reads
when possible.

Use monotonic processing time for windows and rates. Historic log timestamps do
not change ingestion throughput. Attribute records at read-batch granularity.
The window comprises the current partial second and up to W-1 preceding seconds;
publish its actual covered duration, and document that its boundary has one-second
resolution. Idle snapshots expire old buckets. Session rate uses actual elapsed
time. Counts remain exact regardless of timing granularity.

Use one Cargo package with a library and thin executable. Modules: configuration,
framing, parsing, statistics, input, output, runtime, dashboard. Prefer clap,
memchr, serde/serde_json, ratatui/crossterm, and a small Unix readiness dependency.
Pin the project toolchain and dependency lockfile. Keep filesystem identity,
signals, and descriptor readiness in a platform adapter.

## Following and cancellation

Follow names across file replacement, inspect identity and offset, detect observed
truncation, and retry missing paths with bounded polling. Drain the old file to
its observed end, then read the replacement from its beginning. Never join
partial records across generations. When starting at EOF inside an existing
partial line, discard that line's eventual suffix and report the skipped bytes.
Expose following, waiting, complete, stopped, and failed states.

Late writes to a rotated descriptor after handoff and truncate/regrow races can
lose data; this is a measurement tool without durable checkpoints. Document these
limits. Reader and output waits must allow cancellation. Maintain bounded output
buffering and rate-limit repeated diagnostics.

## Outputs

The dashboard displays throughput, severity ratios/counts, unknown fraction, a
sparkline, session totals, source state, pending bytes, and file backlog when
known. Render about five times a second. `q` quits, space freezes only the display,
and `?` shows help. Resize gracefully and restore terminal settings after normal
exit, errors, signals, and unwinding panics.

Plain text and NDJSON publish periodic snapshots and a final report. JSON has a
versioned schema, snapshot/final kind, session identity, elapsed time, counts,
window duration and counts, rates, ratios, diagnostics, and source state.

Prometheus stdout emits one final exposition. Continuous Prometheus operation
requires a metrics file, atomically replaced with a complete exposition each
interval. Include HELP/TYPE metadata, all severity series (including zeros),
cumulative records/bytes/diagnostics, source health, pending bytes, and a snapshot
freshness timestamp. Use bounded labels; no message, request, or session IDs in
labels. Counters reset when the process restarts. The file is compatible with
Node Exporter's textfile collector; stale files can be detected via freshness.

Invalid arguments and fatal I/O errors have nonzero exit codes. Parse failures
are counted rather than fatal. A service manager supervises headless processes.

## Verification and performance

Test framing across arbitrary chunk boundaries, CRLF, invalid bytes, empty and
huge records, exact EOF handling, malformed/nested JSON, and false-positive text
severity. Invariants: severity totals sum to record totals; splitting the same
bytes differently does not change counts; every output uses the same snapshot.

Use deterministic time tests for startup, bucket boundaries, idle expiry, and
zero denominators. Exercise the real executable with fixtures, pipes, rotation,
truncation, disappearing paths, signals, stalled output, and idle stdin. Validate
Prometheus output with promtool when available and the dashboard through both
buffer assertions and an actual pseudo-terminal.

Provisional performance targets, not measured claims:

| Measure | Target |
| --- | --- |
| 200-byte text records | 1 million records/second |
| 500-byte JSON records | 250,000 records/second |
| Default resident memory | Below 64 MiB with no input-volume-dependent growth |
| Dashboard throughput overhead | Below 10 percent |
| Typical display freshness | Within 500 ms |
| Idle and loaded quit responsiveness | Within one second |

Record hardware, toolchain, build flags, fixtures, and input mode. Benchmark
parsing, cached-file scans, and live pipes separately; distinguish disk throughput
from cached reads. Include malformed records, bursts, long lines, and a sustained
run. Optimize only measured bottlenecks. CI covers formatting, clippy, unit and
integration tests on macOS/Linux; do not assert hardware-specific speed in CI.

## Implementation sequence

1. Save this plan; initialize Git.
2. Scaffold Rust package, toolchain, CLI contract, and CI.
3. Add bounded framing with boundary and oversized-input tests.
4. Add severity parsers with representative and adversarial fixtures.
5. Add counters, snapshots, and deterministic rolling-window tests.
6. Add finite input and exact plain-text/JSON reports; record an early benchmark.
7. Add file following, cancellation, and source-health integration tests.
8. Add periodic output, Prometheus exposition, and atomic metrics-file publication.
9. Add dashboard, interactive tests, and terminal restoration.
10. Validate sustained operation, refine measured bottlenecks, and document usage,
    installation, benchmark results, and any remaining limitations.

Each coherent change and its related tests is verified and committed before the
next independent change. Keep the small commits when publishing.

## Later increments

Multiple sources, Windows support, logfmt/numeric severity profiles, an HTTP scrape
endpoint, persistent aggregate history, and a local web interface.

## References

- [GNU tail semantics](https://www.gnu.org/s/coreutils/manual/html_node/tail-invocation.html)
- [Ratatui setup](https://ratatui.rs/installation/)
- [Prometheus exposition](https://prometheus.io/docs/instrumenting/exposition_formats/)
- [Prometheus naming](https://prometheus.io/docs/practices/naming/)
- [Node Exporter textfile collector](https://github.com/prometheus/node_exporter#textfile-collector)

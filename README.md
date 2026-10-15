# Tell-Tail

A Rust terminal dashboard for log throughput and severity. Logs pass through;
Tell-Tail keeps counters and bounded rolling statistics. It follows one file or
reads stdin, and supports plain text, NDJSON, and Prometheus output.

## Try it

The repository pins Rust 1.88.0. With rustup installed:

```sh
git clone https://github.com/zuchka/tell-tail.git
cd tell-tail
cargo build --release --locked
python3 scripts/demo.py | ./target/release/tell-tail
```

The demo changes warning and error ratios over a 20-second cycle. Press `q` to
quit, space to freeze the display while ingestion continues, and `?` for help.

To install the binary locally:

```sh
cargo install --path . --locked
```

## Inputs and output

```sh
# Follow only new records, across rename-based rotation
 tell-tail app.log

# Include existing contents, then follow
 tell-tail app.log --from start

# Scan from the beginning and print a final report
 tell-tail app.log --once

# Read stdout and stderr from another process
 my-app 2>&1 | tell-tail

# Explicit parsing and a five-minute processing-time window
 tell-tail app.log --format json --level-field severity --window 300

# Headless summaries or machine-readable snapshots
 tell-tail app.log --output text
 tell-tail app.log --output json > stats.ndjson

# One Prometheus exposition at EOF
 tell-tail app.log --once --output prometheus

# Continuous atomic snapshots for a textfile collector
 tell-tail app.log --output prometheus --metrics-file tell-tail.prom
```

`--output auto` selects the dashboard for live input when stdout and a controlling
terminal are available and `TERM` is usable. Finite file scans default to plain
text. Force the dashboard with `--output tui`; piped logs still use a separate
terminal descriptor for keyboard input. Dashboard completion restores the screen
and prints a final text report. Small terminals show a compact view.

Text and JSON default to one snapshot per second (`--interval-ms 1000`) plus a
final report. JSON has `schema_version: 2`; ratios are fractions between zero and
one, with `null` for empty denominators. Counts and byte totals are cumulative
since process start; `window` contains recent measurements. Diagnostics use stderr.

Prometheus stdout contains one final document. A metrics file is required for
continuous file following in Prometheus mode. The file is replaced, never
appended, and can be read by Node Exporter's textfile collector. New files have
mode 0644; existing permissions are preserved. The snapshot timestamp detects a
stale file, and `tell_tail_running` is zero on orderly termination. Counters reset
on process restart. An HTTP scrape endpoint is a later increment.

## What is counted

- Each physical line is one record. Blank and unrecognized lines count as unknown.
  A final nonempty line without a newline counts at finite EOF. A trailing newline
  does not create another record. CRLF is supported.
- Severity categories are trace, debug, info, warn, error, fatal, and unknown.
  Names are case-insensitive. Aliases: warning → warn; critical and panic → fatal.
- JSON reads the top-level `level` string, configurable with `--level-field`.
  Missing, numeric, or unrecognized values are unknown. Malformed JSON and invalid
  UTF-8 in JSON increment `parse_failures`. Duplicate severity fields are malformed.
- Text accepts `INFO message`, `[WARN] message`, `ERROR: message`, and an ISO-shaped
  timestamp followed by a level, such as `2026-10-14T22:01:02Z ERROR message`.
  Bracketed timestamps work too. Words elsewhere in a message do not set severity.
- Auto parsing treats lines starting with `{` after whitespace as JSON. Invalid
  JSON never falls back to text. Other formats, ANSI-colored prefixes, and multiline
  event reconstruction remain unsupported in 0.2.
- Warning ratio is warn / all records. Error ratio is (error + fatal) / all records.
  Unknown records remain in both denominators. These are log ratios, not request
  failure rates.

Rates use monotonic processing time, not timestamps embedded in logs. A fast scan
of an old file measures scan throughput. Windows use 100 ms buckets and retain at least the requested duration
once warmed up, with less than 100 ms of extra coverage at the left edge.
`observed_seconds` reports that actual duration, including startup;
`window.resolution_seconds` is 0.1. Counts are exact for the included buckets.
Idle windows expire while session totals remain. The chart shows completed seconds.
See [the 0.2 migration notes](CHANGELOG.md) for the change from schema 1.

## Operational behavior

The input buffer, partial record, time buckets, and latest snapshot are bounded.
`--max-record-bytes` defaults to 1 MiB and accepts up to 16 MiB. Oversized records
are drained, counted once as unknown, and recorded in `oversized_records`.
Consumed byte counts include delimiters and skipped bytes.

Following retries missing files and detects observed truncation. A replacement
file is read from its beginning after draining the old file to its observed end.
Incomplete records are discarded at generation boundaries, with counters. Starting
at EOF inside an existing line skips its later suffix. SIGINT/SIGTERM report
pending bytes without treating the unfinished record as complete.

There are no durable checkpoints: restarts reset statistics. Late writes to an old
rotated file after handoff and truncate/regrow cycles between polls can be missed.
Named file inputs must be regular files. Stdin accepts regular files, pipes,
stream sockets, and `/dev/null`; other devices and datagram sockets are rejected. Backlog is reported
for the current followed descriptor and is unknown for stdin.

A slow output consumer cannot grow a report queue or stop the ingestion worker.
Intermediate snapshots may be skipped. A complete stdout report or terminal frame exceeding two seconds
fails the process; cancellation allows up to 250 ms for output to finish.
Inherited stdout flags remain unchanged. An output failure
may leave an incomplete final report. Metrics-file failures are fatal. Missing
input files are represented by a waiting state rather than repeated diagnostics.

Exit status: 0 on EOF or `q`, 2 for argument errors, 1 for runtime failures,
130 for SIGINT, and 143 for SIGTERM. A closed downstream pipe exits quietly. An output failure takes precedence over
a signal exit code and returns 1; an undrained sink may prevent its diagnostic
from being displayed.
A service manager can supervise long-running headless processes. See the
[Linux service example and troubleshooting guide](docs/OPERATIONS.md).

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked
python3 scripts/check_terminal.py
python3 scripts/check_backpressure.py
cargo build --release --locked
python3 scripts/benchmark.py
python3 scripts/soak.py --seconds 60
python3 scripts/soak.py --seconds 60 --tui
```

CI runs on macOS and Linux. Parsing, framing, and statistics are portable; Unix
file identity, readiness, signals, and terminal I/O are isolated for future
Windows support. See [the implementation plan](docs/PLAN.md) for scope and
subsequent increments, and [validation results](docs/VALIDATION.md) for measured
performance and test coverage.

The [0.2 implementation plan](docs/PLAN-0.2.md) defines the next reliability
milestone, focused commits, and release criteria.

The [0.2 implementation validation](docs/VALIDATION-0.2.md) records completed
checks and remaining release gates. See [validation commands](docs/TESTING-0.2.md)
for the expanded test, fuzz, soak, benchmark, and package workflows.

## License

Tell-Tail is licensed under the [MIT License](LICENSE).

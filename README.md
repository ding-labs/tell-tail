# Tell-Tail

A Rust log throughput and severity monitor with terminal and local web dashboards.
Tell-Tail reads one file or stdin and keeps counters, bounded rolling statistics,
and a small in-memory sample of recent errors for its dashboards. It supports
plain text, NDJSON, Prometheus, and a local HTTP/SSE metrics API.

Hosted documentation: [released guides](https://docs.ding.ing/tell-tail/) ·
[unreleased preview](https://docs.ding.ing/tell-tail/next/) ·
[Ding docs](https://docs.ding.ing/).

## Install and try it

Install Tell-Tail through Homebrew or
[crates.io](https://crates.io/crates/tell-tail):

```sh
brew install ding-labs/tap/tell-tail
# Or build with Cargo (requires Rust 1.88 or later):
cargo install tell-tail --locked
```

Follow your application's log file in the terminal or browser:

```sh
tell-tail app.log
tell-tail app.log --output web --open
```

Following starts at the end of the file. If nothing new is being written, an
empty display is expected; add `--from start` to include existing contents.
For a quick example without a log file, this command leaves its final report
open in the browser until Ctrl+C:

```sh
printf 'INFO ready\nWARN upstream slow\nERROR upstream timeout\n' | tell-tail --output web --open
```

Press `q` to quit the terminal dashboard, space to freeze its display while
counting continues, and `?` for help. Arrow keys and Page Up/Down freeze and
scroll recent errors; `f` toggles fatal-only review. At 60 × 20 or larger, the
terminal shows up to 20 error/fatal previews, each capped at 512 bytes and kept
only in memory. Long previews are marked with an ellipsis. Previews are omitted
from CLI JSON, text, Prometheus, and webhook payloads.

[GitHub Releases](https://github.com/ding-labs/tell-tail/releases/tag/v0.3.0)
provides native archives for Apple Silicon on macOS 14+ and static Linux binaries
for x86_64 and ARM64. The dashboard is embedded; the archives need no Rust, Node,
or Python. See the [distribution guide](docs/RELEASING.md) for platform details.

The examples below target **0.3.0 or later**. Upgrade an existing Homebrew
installation with `brew upgrade ding-labs/tap/tell-tail`, or reinstall with Cargo.
Check `tell-tail --version` before trying the demo or parser profiles.

Read [Understanding a live log stream](docs/MONITORING.md) for a complete guide
to interpreting coverage, history, activity ages, controls, and their limits.
The [documentation index](docs/README.md) links all user guides, API contracts,
troubleshooting, and validation procedures.

### MCP diagnostics (unreleased)

Watch existing Codex Desktop and Claude Desktop MCP logs on macOS:

```sh
cargo run --release --locked -- mcp --open
# Synthetic failures and recovery; reads no client logs:
cargo run --release --locked -- mcp --demo --open
```

The local dashboard groups reported failures by server and execution, shows a
live searchable timeline, and distinguishes history, recovery, and source
freshness. No server configuration change, plugin, or account is needed.
After installing this checkout, the command is `tell-tail mcp --open`.
See [MCP diagnostics](docs/MCP.md) for sources, headless output, and coverage limits.
The [hosted MCP guide](https://docs.ding.ing/tell-tail/next/mcp/) belongs to the
unreleased preview. It follows a pinned source revision and may lag this checkout;
MCP is not in the released guides yet.

### Built-in demo

Try either dashboard without a log file or generator:

```sh
tell-tail --demo --window 5
tell-tail --demo --window 5 --output web --open
```

The source is labeled synthetic. Its 40-second cycle has eight seconds of normal
activity, eight with warnings, eight with errors, six without records, and ten
of recovery. Active phases generate approximately 100 records/second. The
five-second rolling window makes the transitions easy to see; histories still
cover 60 completed seconds. Press `q` in the terminal or Ctrl+C to stop. The demo
uses the normal ingestion path and does not read stdin or create log files.

It now mixes text (including Python prefixes and ANSI colors), JSON, logfmt, and
syslog. During the warning/error phases, deliberate numeric and conflicting-field
examples show why some records stay unknown. Expand **Understand parsing coverage**
in the browser or press `?` in the terminal to see parser evidence and flag hints.
To try a numeric convention without preparing input, add `--profile pino`:

```sh
tell-tail --demo --profile pino --window 5 --output web --open
```

Demo profile/field/path flags generate matching JSON samples; envelope flags wrap
them. The same flags on real input only configure parsing. No config file is needed.

For development, clone the repository and use
`cargo run --locked -- --demo --window 5 --output web --open`.

The [demo walkthrough](docs/DEMO.md) explains every phase, what to observe,
supported options, shutdown behavior, and how to distinguish it from a benchmark.
When ready, [keep the flags and switch to your own logs](docs/DEMO.md#move-from-the-demo-to-your-logs).

## Everyday workflows

Replace the example filenames and commands with your own:

```sh
# Watch a service while deploying or debugging it
tell-tail app.log --window 10 --output web --open

# Observe a Python worker with unbuffered stdout and stderr
python3 -u worker.py 2>&1 | tell-tail

# Read JSON logs whose string severity lives in a different top-level field
tell-tail worker.ndjson --level-field severity

# Review a completed capture and print its final counts
tell-tail capture.ndjson --once
```

If a piped program buffers its output, configure that program to flush promptly;
Tell-Tail can only measure records once they arrive. A fast scan of an old file
shows processing throughput, not its original event rate. No recent logs alone
cannot establish whether the application is healthy or stalled.

## Log formats with minimal setup

Tell-Tail recognizes common JSON severity fields, logfmt key/value records,
Python-style console prefixes, timestamped/colored levels, and syslog priority
headers automatically. Recognition runs per record, so supported styles can mix.

```sh
my-app 2>&1 | tell-tail --output web --open
# An explicit numeric convention needs just one flag:
node app.js 2>&1 | tell-tail --profile pino
# Choose a literal key or a nested JSON object path:
tell-tail app.log --level-field priority
tell-tail app.log --level-path /log/level
# Unwrap a saved container capture, then classify its application records:
tell-tail saved-cri.log --envelope cri --profile pino --from start
```

Other JSON profiles are `python` (numeric `levelno`), `otel` (one JSON LogRecord),
and `serilog` (compact JSON). Numeric conventions are not guessed. Explicit
selectors override automatic field selection; conflicting automatic candidates
remain unknown. Browser hints and terminal `?` help explain recognized rule counts
and corrective flags. See the [format reference](docs/LOG-FORMATS.md) for exact
mappings, examples, compatibility changes, container limits, and troubleshooting.

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

## Local web dashboard

```sh
tell-tail app.log --from start --output web --open
my-app 2>&1 | tell-tail --output web
```

Web mode serves an embedded live dashboard and prints its loopback URL to stderr.
It shows rolling rates and ratios, exact session totals, a 60-second throughput
chart, severity counts, and ingestion diagnostics. It also shows
parsing coverage, warning/error histories, and activity ages as described above. Freeze/resume affects only
the display; connection and failure status stay live. Hidden tabs release their
stream and reconnect when shown. The binary works offline without Node or a CDN.
The compact layout also shows the latest 20 error/fatal previews with a fatal-only
filter. Freeze holds these previews for review; each is capped at 512 bytes and
retained only in memory.

`--open` optionally launches the default browser. Use
`--web-port 8765` for a fixed port; otherwise the OS selects one. It writes no
stdout snapshots. Normal stdin EOF keeps final metrics available until Ctrl+C;
`--once --output web` is rejected. Four live streams are supported; additional
tabs show a capacity message and a retry button. JSON and SSE are also available
at `/api/snapshot` and `/api/events`.

On macOS, stream real system activity directly into the dashboard:

```sh
tell-tail --apple-unified --output web --open
```

Tell-Tail starts `/usr/bin/log stream` at info level, maps Apple's message types,
and skips stream metadata. No pipe, `jq`, or saved log file is required. Ctrl+C
stops the capture process and dashboard. Omit `--output web --open` for the
terminal dashboard, or use `--output json` for metrics snapshots.

For an existing Apple NDJSON capture, select its format explicitly:

```sh
tell-tail capture.ndjson --format apple-unified --once
```

Saved NDJSON parsing works on macOS and Linux; live `--apple-unified` capture is
macOS-only. See [Apple unified log input](docs/APPLE-UNIFIED.md) for levels, metadata,
byte accounting, and capture lifecycle. To try a predictable continuous demo, run
`python3 scripts/demo.py --rate 1000 | tell-tail --output web --open`
from this repository.

Web support is included by default. Build with `--no-default-features` for the
CLI-only binary. See the [dashboard guide](docs/WEB-DASHBOARD.md),
[server behavior and limits](docs/WEB-SERVER.md), and the
[web JSON contract](docs/WEB-FRAMES.md); CLI JSON schema 2 is unchanged.

## Webhook alerts

Send a JSON message when errors reach 25% of records in the rolling window,
and a recovery notification when they fall below it. Configure these options
when starting Tell-Tail; the receiver must accept JSON POSTs and return HTTP 2xx:

```sh
tell-tail app.log --window 60 \
  --alert-error-percent 25 \
  --alert-webhook https://example.com/hooks/tell-tail \
  --alert-message "Check the service: errors reached 25%"
```

For Slack, add `--slack` and use your Slack incoming webhook URL:

```sh
tell-tail app.log --slack \
  --alert-webhook "$SLACK_WEBHOOK_URL" \
  --alert-error-percent 25
```

To get a URL, [create a Slack app and enable Incoming Webhooks](https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/),
then choose **Add New Webhook to Workspace**, select the destination channel,
and copy the URL into `SLACK_WEBHOOK_URL`. The webhook determines the channel;
Tell-Tail needs no bot token or formatting relay.

Slack messages include breach/recovery status, the message, error percentage,
error/fatal and total record counts, threshold, window, source, and session ID.
For example, a breach message looks like:

```text
🚨 Tell-Tail alert: error threshold reached
Tell-Tail error percentage reached its threshold.
Errors + fatal: 25.00% (25 / 100 records); threshold: 25%.
Window: 60s (60.0s observed).
Source: app.log
Session: 1234-1792010000000000000
```

`--alert-message` replaces the default breach message. Recovery has its own
status and fixed message. Message text is displayed literally, with Slack
formatting and link previews disabled; source names longer than 1024 characters
are shortened. The same threshold, minimum samples, cooldown, and delivery
limits apply.

For Discord, add `--discord` and use your Discord incoming webhook URL:

```sh
tell-tail app.log --discord \
  --alert-webhook "$DISCORD_WEBHOOK_URL" \
  --alert-error-percent 25
```

In Discord's **Server Settings → Integrations**, [create a webhook](https://support.discord.com/hc/en-us/articles/228383668-Intro-to-Webhooks),
choose the destination text channel, and use **Copy Webhook URL** to set
`DISCORD_WEBHOOK_URL`. Tell-Tail sends directly to that URL; no bot token or
formatting relay is needed.

Discord notifications use a red embed for breaches and a green embed for
recoveries, with the message, error percentage and counts, threshold, window,
source, and session ID. The full `--alert-message` (up to 4096 bytes) is included
in the breach embed; recovery uses its own fixed message. Discord text formatting
is supported, and user, role, and everyone mentions are disabled. Long source
names are shortened to fit Discord's field limit. The same alerting and
best-effort delivery rules below apply.

Choose one of `--slack` or `--discord` for a single endpoint. Omit both to send
the generic JSON payload below.

For a quick demo from a source checkout, run:

```sh
cargo build --locked
python3 scripts/demo_webhooks.py
```

The [demo](scripts/demo_webhooks.py) starts a temporary local webhook receiver
and runs the real binary through healthy logs (0% errors), a breach (50%), and
recovery (20%). It prints excerpts from both received JSON payloads, acknowledges
them with HTTP 204, and exits. Python 3 and the built binary are the only runtime
requirements. Use `--binary /path/to/tell-tail` to select a different binary.
Recovery arrives during the default five-minute breach cooldown.

For continuous testing with resource limits and optional Slack delivery, see the
[24-hour webhook validation guide](docs/WEBHOOK-SOAK.md). That harness captures
and validates generic alert JSON, then uses its own relay for Slack delivery.

The [October 22–23, 2026 run](docs/WEBHOOK-SOAK.md#reviewed-october-23-result-passed)
passed 24 uninterrupted hours at 1,000 records/second under one CPU, 1 GiB RAM
and no swap. All 86.4 million records matched the expected counters and bytes,
and all 24 breach/recovery pairs reached Slack. Peak sampled monitor/helper
memory was 7.95 MiB; peak memory for the whole test container was 42 MiB.

The rule uses `(error + fatal) / all records`, including unknown records in the
denominator. It evaluates each ingestion snapshot (roughly every 100 ms) and at
finite EOF, independently of the display/publication interval. It works with
stdin, files, every output mode, and a frozen dashboard display.

- `--alert-error-percent`: inclusive threshold from 0 to 100. Requires an endpoint.
- `--alert-webhook`: one HTTP(S) endpoint, up to 2048 bytes. Requires a threshold.
- `--slack`: format notifications for a Slack incoming webhook. Requires
  `--alert-webhook` and `--alert-error-percent`.
- `--discord`: format notifications for a Discord incoming webhook. Requires
  `--alert-webhook` and `--alert-error-percent`; cannot be combined with `--slack`.
- `--alert-message`: optional breach text, up to 4096 bytes, included as a JSON
  string. Recovery uses its own fixed message.
- `--alert-min-records`: minimum records in the window; default **100**. A partial
  startup window can qualify. Set this to 1 for very small test streams.
- `--alert-cooldown`: minimum seconds between new breaches; default **300**.
  Accepts 0 through 86400.

A sustained breach fires once. After a breach is queued, the first eligible
window strictly below the threshold sends one recovery notification and rearms
the rule when that notification is queued. Recovery requires the same minimum
record count as a breach. It bypasses the cooldown and does not restart it;
subsequent breaches can fire once the cooldown since the last queued breach
expires. A breach suppressed by cooldown does not produce a recovery notification.
Initially healthy, empty, and undersized windows do not send recovery. Stopping
the process or an input failure does not signal recovery. State resets when
Tell-Tail restarts. There are no repeated reminders. With a threshold of 0,
eligible windows always breach and cannot recover.

The generic webhook payload includes your message and aggregate statistics:

```json
{
  "schema_version": 1,
  "event": "error_threshold_met",
  "message": "Check the service: errors reached 25%",
  "session_id": "1234-1792010000000000000",
  "source": "app.log",
  "observed_at_unix_seconds": 1792010060.0,
  "window_seconds": 60,
  "observed_seconds": 60.0,
  "records": 100,
  "error_records": 25,
  "error_percent": 25.0,
  "threshold_percent": 25.0
}
```

Recovery uses the same payload shape with `event: "error_threshold_recovered"`
and `message: "Tell-Tail error percentage recovered below its threshold."`.
Its statistics describe the recovered window; `session_id` and `source` identify
the same monitoring session as the breach. Receivers should distinguish the two
event values rather than treating every message as a breach.

Webhook schema 1 is separate from the existing snapshot schema. Percentages are
on a 0–100 scale. No raw log records or endpoint URL are included in either payload.

Delivery uses `POST` with `Content-Type: application/json`; any HTTP 2xx response
counts as success. HTTPS verifies certificates. Redirects are not followed, and
URL userinfo/fragment syntax is rejected. Custom headers and arbitrary JSON
templates are outside this first version.

Delivery is best effort: one active request and one queued event, with a
five-second request deadline. A full queue leaves the current transition eligible
for evaluation on a later qualifying snapshot without changing the rule state.
There are no automatic retries or durable storage, so transient breaches,
recoveries, or failed deliveries can be lost. State changes when a notification
is queued, even if delivery later fails; a recovery can therefore arrive when
the receiver missed its breach. Failures write a
sanitized diagnostic to stderr and do not change log counts or the exit status.
Shutdown allows 250 ms for delivery, then cancels pending work; even a finite scan
can exit before a slow receiver acknowledges its final alert. Network I/O never
runs on the ingestion thread. Alerting is disabled unless configured.

## What is counted

Both dashboards show **severity recognition coverage** for the rolling window:
records with a recognized severity divided by all records. This is a measured
coverage fraction, not confidence in application health. An empty window shows
no percentage. The web dashboard breaks unknown records down into malformed,
oversized, and records without a recognized severity, with parsing guidance;
press `?` in the terminal for the supported prefix/field conventions.

Throughput, warning, and error-plus-fatal histories show the last 60 completed
seconds of processing time. Severity charts share a count scale, so a change in
severity remains visible even when total volume is steady. The web dashboard
also compares error counts and ratios over the last 30 completed seconds and
the preceding 30, after a full minute is available. These intervals are separate
from `--window`; empty intervals have no error percentage.

Activity freshness shows the time since the last complete record and the last
error/fatal record, measured at the displayed snapshot using processing time.
Partial input bytes do not reset these ages; a final unterminated record counts
at finite EOF. Ages freeze with the display and stop at the final snapshot.
“None observed” means none in this session, not that the application is healthy.
When a followed file has no new records, use `--from start` to include its existing
contents. Missing files and incomplete lines have separate waiting guidance.

- Each physical line is one record. Blank and unrecognized lines count as unknown.
  A final nonempty line without a newline counts at finite EOF. A trailing newline
  does not create another record. CRLF is supported.
- Severity categories are trace, debug, info, warn, error, fatal, and unknown.
  Names are case-insensitive. Aliases: warning → warn; critical and panic → fatal.
- Tell-Tail recognizes common JSON keys such as `level`, `severity`, and
  `log.level`. `--level-field` selects one literal key; `--level-path` selects a
  nested object value. Duplicate selected keys are malformed; conflicting
  automatically selected values remain unknown. Named profiles map numeric levels.
- Text accepts `INFO message`, `[WARN] message`, `ERROR:message`, Python prefixes,
  and ISO-shaped timestamps with `T` or a space. Bounded ANSI SGR colors are
  ignored for classification. Message keywords never set severity.
- Auto mode selects JSON, logfmt, syslog priority headers, or leading-level text
  per record. Malformed structured input never falls back to message guessing.
  Multiline event reconstruction and arbitrary custom layouts remain unsupported.
  Published 0.2.0 has the earlier, narrower parser; see the format reference above.
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
python3 scripts/check_terminal.py --alerts
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

The [0.2 implementation plan](docs/PLAN-0.2.md) records that release's reliability
scope, focused increments, and validation criteria. For current dashboard work,
use the [feature validation guide](docs/TESTING-DASHBOARDS.md).

The [0.2 implementation validation](docs/VALIDATION-0.2.md) records checks and
open items at that milestone; the [release validation](docs/VALIDATION-0.2-RELEASE.md)
records the later release decision. See [validation commands](docs/TESTING-0.2.md)
for the expanded test, fuzz, soak, benchmark, and package workflows.

## License

Tell-Tail is licensed under the [MIT License](LICENSE).

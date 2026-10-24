# Tell-Tail

A Rust log throughput and severity monitor with terminal and local web dashboards.
Logs pass through; Tell-Tail keeps counters and bounded rolling statistics. It
follows one file or reads stdin, and supports plain text, NDJSON, Prometheus, and
a local HTTP/SSE metrics API.

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
The terminal uses Ding's graphite and green palette with a compact chart/severity
layout, violet information, amber warnings, and coral errors/fatals. At 60 × 20
or larger it also shows recent error and fatal previews, newest first. Arrow keys
and Page Up/Down freeze the display and scroll the previews; `f` toggles fatal-only
review, and space resumes live updates. The dashboard retains at most 20 previews
of 512 bytes each in memory; long previews are marked with an ellipsis. These
previews are omitted from CLI JSON, text, Prometheus, and webhook payloads.

To install the binary locally:

```sh
cargo install --path . --locked
```

Release automation and Homebrew packaging are prepared, but no release has been
published yet. Tell-Tail will join `ding` in the existing
[Ding Labs tap](https://github.com/ding-labs/homebrew-tap) after the remaining 0.2
validation passes. The planned command is `brew install ding-labs/tap/tell-tail`;
it is not available yet. See the [release workflow instructions](docs/PLAN-0.2.md#release-automation).

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
chart, severity counts, and ingestion diagnostics. Freeze/resume affects only
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

The [0.2 implementation plan](docs/PLAN-0.2.md) defines the next reliability
milestone, focused commits, and release criteria.

The [0.2 implementation validation](docs/VALIDATION-0.2.md) records completed
checks and remaining release gates. See [validation commands](docs/TESTING-0.2.md)
for the expanded test, fuzz, soak, benchmark, and package workflows.

## License

Tell-Tail is licensed under the [MIT License](LICENSE).

# Understanding a live log stream

This guide covers parsing coverage, severity history, activity freshness, and expanded format recognition in
the current source checkout. These additions are **unreleased** and are not in
the published 0.2.0 Homebrew, Cargo, or release archives. See the
[installation instructions](../README.md#install-and-try-it) and the
[built-in demo walkthrough](DEMO.md) to run the new dashboard locally.

Tell-Tail helps answer four questions: are records arriving, can their severity
be recognized, is the severity mix changing, and which recent errors can I
inspect? It measures physical log records, not requests, successful jobs, or
application health. It does not infer root causes or predict incidents.

## Start with your own logs

From the repository root, build the checkout once:

```sh
cargo build --locked
```

The commands below use that binary, avoiding an older installation on `PATH`.
Replace example files and programs with your own.

```sh
# Watch new records in a file using the terminal dashboard
./target/debug/tell-tail app.log

# Include existing contents, then follow new records in a browser
./target/debug/tell-tail app.log --from start --output web --open

# Watch a Python worker; -u prevents Python's stdout buffering
python3 -u worker.py 2>&1 | ./target/debug/tell-tail --window 10

# Select a top-level string severity field in JSON records
./target/debug/tell-tail worker.ndjson --level-field severity

# Count a completed capture and print a final report
./target/debug/tell-tail capture.ndjson --once
```

One process observes one file or stdin. `--apple-unified` is a separate macOS
source; see [Apple unified logs](APPLE-UNIFIED.md). File following starts at the
end unless `--from start` is supplied. Reading existing content may produce a
large initial throughput spike because history uses the time of processing.

`--output auto` uses the terminal dashboard when stdout and a controlling
terminal are available; redirected output normally becomes text. Use
`--output web` for a browser. `--open` only launches the browser; the printed URL
also works manually. A finite stdin stream leaves the web server showing final
metrics until Ctrl+C. `--once --output web` is not supported.

## Three different time scopes

| Scope | What belongs to it | Duration |
| --- | --- | --- |
| Session | Cumulative records, bytes, severity counts, and last-observed activity | Since this process started |
| Rolling window | Rates, warning/error ratios, recognition coverage, and window counts | `--window`, default 60 seconds, range 1–3600 |
| Completed-second history | Throughput, warnings, and errors plus fatal per second | Up to the most recent 60 completed seconds |

All three use monotonic **processing time**, not timestamps embedded in logs.
A scan of yesterday's file measures today's scan speed. A producer that buffers
output can make old events arrive in a burst; Tell-Tail cannot measure them
before they arrive.

Rolling windows use 100 ms buckets. After warmup, they retain the configured
duration plus less than 100 ms at the oldest edge. Rates divide by the displayed
observed duration, including during startup. A `5.1s observed / 5s configured`
label can result from display rounding; it does not mean a five-second window
has silently become a different setting.

## Parsing coverage: what the percentage means

The UI calls this **severity recognized**. It is a measured coverage fraction,
not a model probability or an assessment that the application's logs are correct.

```text
recognized records = all records - unknown records
recognition coverage = recognized records / all records
warning ratio = warn records / all records
error ratio = (error records + fatal records) / all records
```

These calculations use the rolling window. Unknown records remain in every
denominator. An empty window has no coverage or severity ratio; it is not 0%.
The web view says coverage is unavailable, and the terminal shows an em dash.
Percentages are rounded for display; exact counts can reveal unknown records
even when the displayed coverage rounds to 100%.

For example, suppose the window contains:

| Category | Records |
| --- | ---: |
| Info | 70 |
| Warn | 10 |
| Error | 5 |
| Fatal | 1 |
| Unknown | 14 |
| Total | 100 |

Coverage is 86%, warning ratio is 10%, and error-plus-fatal ratio is 6%.
If three unknown records are malformed and two are oversized, the remaining
nine lack a recognized severity. Those categories explain the same 14 unknown
records; they are not additional records. Conversely, 100% coverage could
describe a stream consisting entirely of errors.

In the browser, expand **Understand parsing coverage** for the rolling breakdown
and supported-format guidance. **Ingestion diagnostics** shows session totals,
so its parse-failure count can remain nonzero after rolling failures expire.
The full terminal puts rolling coverage in the Severity panel title; the compact
view labels it explicitly. Press `?` for format guidance.

### Why a record is unknown

| Category | Examples | Interpretation |
| --- | --- | --- |
| Malformed | Invalid structured input, invalid UTF-8 in JSON/logfmt, duplicate selected severity fields, invalid Unicode escapes | Also increments `parse_failures` |
| Oversized | A physical line exceeds `--max-record-bytes` | Drained and counted once as unknown; increments `oversized_records` |
| No recognized severity | Blank lines, missing or unsupported level values, numeric JSON levels without a matching profile, unrecognized text prefixes | May be valid application output; does not itself increment `parse_failures` |

The default record limit is 1 MiB and the maximum is 16 MiB. Increasing it raises
buffer capacity. Parsing coverage adds no retained raw-log archive. For supported parsers and
explicit profiles, see the [format reference](LOG-FORMATS.md).

### Supported input examples

With `--format auto` and no field/profile override:

| Record | Result |
| --- | --- |
| `INFO request completed` | Info |
| `[WARN] upstream slow` | Warn |
| `ERROR: upstream timeout` | Error |
| `2026-11-05T12:00:00Z ERROR upstream timeout` | Error |
| `{"level":"warning","message":"retrying"}` | Warn |
| `{"level":"panic","message":"failed"}` | Fatal |
| `{"level":50,"message":"timeout"}` | Unknown, not a parse failure |
| `{"severity":"error"}` | Error; a common field is recognized automatically |
| `{"context":{"level":"error"}}` | Unknown; select `--level-path /context/level` |
| `request completed with ERROR in its message` | Unknown; words inside the message do not set severity |
| `{bad}` | Unknown plus a parse failure |

Severity names are case-insensitive. `warning` maps to warn; `critical` and
`panic` map to fatal. Auto mode treats a record starting with `{` after whitespace
as JSON; malformed JSON does not fall back to text. `--format json` explicitly
requires JSON. Numeric mappings require a named profile; nested selectors use
`--level-path`. Explicit `--level-field` names a literal top-level key.

Current main also recognizes bounded ANSI SGR colors, logfmt fields, and syslog
priority headers. Explicit profiles and nested paths handle other conventions;
see [format recognition](LOG-FORMATS.md). Multiline reconstruction remains
unsupported. A multiline stack trace consists of
multiple physical records; continuation lines often have unknown severity.
Apple captures have their own [explicit format rules](APPLE-UNIFIED.md).

## Severity history: what changed while you looked away?

The throughput chart counts all records in each completed second. The severity
charts count warnings and combined errors/fatals over those same intervals.
They show counts per second, not percentages or the rolling average rate.
Unknowns contribute to throughput but not to either severity series.

At 12.4 seconds elapsed, history covers completed intervals from `[0,1)` through
`[11,12)`. The current `[12,13)` interval appears after it completes. History is
empty during the first second and grows to 60 points. Idle seconds are zero;
after a long idle, old spikes disappear from history while session counts remain.
Changing `--window` does not change the history length or bucket size.

The warning and error plots share a count scale with each other. Throughput has
its own scale. A steady throughput line with rising error counts can reveal a
severity change that volume alone hides. A rising error percentage without more
errors may instead reflect fewer informational records, so inspect the counts.

In the browser, warnings use an amber dashed line and errors/fatals a coral solid
line. **View exact severity values** lists their counts and elapsed intervals.
In the terminal, `Warn/s` and `Err+fatal/s` identify the two sparklines; the title
value is the latest completed second. Narrow terminals display only the newest
history points that fit, and compact mode omits charts.

### Browser comparison: two adjacent 30-second intervals

After 60 completed seconds, the browser compares the last 30 completed seconds
with the preceding 30. At 75.6 seconds elapsed, those intervals are `[45,75)` and
`[15,45)`. Each interval's ratio divides its combined error/fatal count by its
own total records. It does not average individual second percentages.

For example, `60 / 300 (20%); preceding 30: 0 / 300 (0%)` reports both the change
and its sample sizes. `0 / 0 (no records)` is an empty interval, not evidence of
recovery. Before a full minute exists, the comparison reports that it needs 60
completed seconds. The comparison is currently browser-only and independent of
the configured rolling window. Restarting the process resets its warmup.

## Activity freshness: has anything arrived recently?

**Last record** is the age of the most recently completed, counted physical
record, including unknown and oversized records. **Last error or fatal** is the
age of the most recent record classified as either error or fatal. They use
processing time and persist even when the event leaves the rolling window.

| Situation | Effect on activity |
| --- | --- |
| Bytes arrive without a complete newline-terminated record | Ages do not reset; pending bytes may grow |
| A complete INFO record arrives | Last record resets; last error does not |
| A complete ERROR or FATAL record arrives | Both ages reset |
| A final nonempty unterminated record reaches finite EOF | Counted at EOF; ages update according to its severity |
| An incomplete record is discarded at rotation or truncation | Does not count as a completed record |
| Apple stream metadata is skipped | Does not reset record ages |
| Rolling counts expire | Ages continue increasing in live snapshots |
| The process restarts | Both start with no observations |

“None observed” means no matching record in this session. “Unavailable” in the
browser means the frame did not supply activity data, as with an older server.
It is distinct from zero seconds ago. Ages are captured with each snapshot:
freezing, a final frame, or retaining a stale frame also holds the displayed ages.
The browser does not advance them with its own wall clock. Check its connection
banner before treating a displayed age as current.

Quiet logs do not prove that a process has stopped or recovered. A quiet worker,
a buffering producer, a stalled process, and a broken upstream pipe can look
similar at the record level. Inspect the input state and underlying process.

## Reviewing and controlling the display

| Action | Terminal | Browser |
| --- | --- | --- |
| Freeze/resume displayed metrics | Space | Freeze/Resume display button; Tab to it and press Space or Enter |
| Review recent errors | Up/Down, Page Up/Page Down, Home/End; scrolling freezes the snapshot | Freeze, then scroll the preview list |
| Show only fatal previews | `f` | Fatal only button |
| Show/hide terminal help | `?`; Escape closes help | Parsing and diagnostic disclosures provide help |
| Stop monitoring | `q` or Ctrl+C | Ctrl+C in the launching terminal; closing a tab leaves monitoring running |

Freeze does not pause ingestion, counts, or configured alert evaluation. Resuming
shows the latest snapshot; it does not replay each intervening display update.
Recent error previews are capped at 20 entries of 512 bytes each, newest first.
They are neither all historical errors nor a ranking of recurring errors. The
fatal-only filter applies only to that retained sample. Shortened previews are
marked; long terminal lines may also be clipped to fit the display width.

The full terminal layout starts at 60 columns by 18 rows; previews appear at
60 by 20 or larger. More space reveals more previews and footer text. Below the
full-layout size, compact mode shows a subset of metrics, with text clipped to
the available geometry. A 100 by 26 terminal is a useful starting size.

The browser keeps connection and failure banners live even while its metrics are
frozen. If EOF occurs while frozen, resume to see final metrics. The terminal
exits its live view on final input and prints a final text report. See the
[browser guide](WEB-DASHBOARD.md) for reconnect, capacity, and hidden-tab behavior.

## Outputs and limits

Coverage is derived from existing counts. The new severity history and activity
fields are dashboard additions: they do not change CLI JSON schema 2, text
reports, Prometheus metrics, or webhook payloads. Machine consumers can derive
coverage from the rolling counts already present in JSON. The
[web frame contract](WEB-FRAMES.md) describes the additional web fields.

History is bounded, not an export of raw logs. No persistent history, selective
log forwarding, inference service, or automatic incident capture is enabled by
these features. Existing configured webhooks send aggregate breach/recovery
notifications; the new comparison and freshness displays do not create new
alert rules. See [operations and troubleshooting](OPERATIONS.md) for input,
rotation, buffering, and output-failure details.

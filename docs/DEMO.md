# Built-in demo walkthrough

`--demo` requires Tell-Tail **0.3.0 or later**. The demo generates a small synthetic
log stream inside Tell-Tail and sends it through the same framing, parsing,
statistics, preview, and output paths used for real input.

It needs no application, saved log file, Python, Node, account, or external
inference service. Web output serves its bundled dashboard on loopback. No
webhook is contacted unless you explicitly configure one.

## Install and start

Install or upgrade using the [installation guide](../README.md#install-and-try-it),
then launch either dashboard:

```sh
# Terminal dashboard; stop with q or Ctrl+C
tell-tail --demo --window 5

# Browser dashboard; stop with Ctrl+C in the launching terminal
tell-tail --demo --window 5 --output web --open
```

Run one dashboard command at a time. The source label is
`demo (synthetic · 40s cycle)`. The phases describe generated examples, not an
assessment of your machine or any actual service. The demo runs offline.

`--open` is optional. Without it, visit the URL printed to stderr. Use
`--web-port 8765` for a fixed port if it is free. Browser output requires the
default web feature; a CLI-only build still supports terminal and machine output.

For development, the repository pins Rust 1.88.0. From its root, run
`cargo run --locked -- --demo --window 5 --output web --open`, or build once with
`cargo build --locked` and substitute `./target/debug/tell-tail` below. The first
build may download dependencies. `cargo install --path . --locked` installs the
checkout. If a flag is rejected, check `command -v tell-tail` and
`tell-tail --version`: an older binary may precede the new installation on `PATH`.
Published 0.2.0 lacks the demo and parser profiles.

## What happens during each cycle

The default mixed-format cycle repeats every 40 seconds while the process runs:

| Elapsed position in cycle | Generated records | What to look for |
| --- | --- | --- |
| 0–8 seconds | Info across text, JSON, logfmt, and syslog | Coverage is 100%; expand parsing coverage to see recognized rule counts |
| 8–16 seconds | About 60% info, 20% warn, 20% unknown | Warning history rises; numeric and conflicting-field hints appear; coverage settles near 80% |
| 16–24 seconds | About 10% info, 20% warn, 50% error, 20% unknown | Error ratio rises; differently formatted timeout previews appear; coverage stays near 80% |
| 24–30 seconds | No records | Last-record age increases and the rolling window drains |
| 30–40 seconds | Mixed-format info only | Coverage returns to 100% as unknowns expire; session parser hints and earlier error previews remain |

Phase boundaries are relative to the demo's start, not wall-clock timestamps.
The severity mix follows a ten-record pattern; counts near boundaries can differ
slightly from the proportions in the table. Active phases schedule a record
roughly every 10 ms, targeting approximately 100 records/second. Scheduling and
processing overhead can reduce the observed rate. A delayed process does not
build a queue of missed demo records to replay later.

The demo's pace is not a throughput benchmark or a guarantee of records per
second. Use the [benchmark and validation tooling](TESTING-0.2.md) for measured
performance workloads. The separate `scripts/demo.py` generator also remains
available for configurable rates; its 20-second cycle differs from this demo.

## What the parsing examples teach

The default demo uses automatic parsing independently on every record:

| Example shape | Result | Lesson |
| --- | --- | --- |
| `INFO request completed` | Info | A leading level is enough |
| `INFO:worker:request completed` | Info | Python console prefixes work automatically |
| ANSI-colored `ERROR` prefix | Error | SGR colors do not hide severity; previews sanitize escape bytes |
| `{"severity":"warning"}` or `{"levelname":"INFO"}` | Warn / info | Common JSON keys need no override |
| `{"log.level":"info"}` | Info | A common literal dotted key is recognized |
| `level=info msg="ERROR is only a message word"` | Info | Message keywords do not override the level |
| `<14>1 ... ERROR is only a message word` | Info | Syslog priority supplies severity, not message text |
| `{"level":50}` | Unknown | Numeric conventions differ; choose an explicit profile |
| `{"level":"info","severity":"error"}` | Unknown | Select the authoritative field explicitly |

The two intentional unknown shapes appear once each per ten records during the
warning/error phases. They are valid JSON, so **parse failures stay zero** at the
default record limit. They remain in severity-ratio denominators. This is a small
teaching sample, not a claim about real-world format coverage.

The browser shows both numeric and conflicting-field hints; terminal help shows
a prioritized hint (conflicts first). Recognized-rule and unresolved-issue counts
are session totals. Rolling coverage can return to 100% during recovery while
session hints remain. No automatic profile switch happens midway through a demo.

## A guided first minute

1. Start with `--window 5`. During the first second, the histories are still
   waiting for a completed interval. Rolling counters can already be nonzero.
   Expand **Understand parsing coverage**: text, JSON, logfmt, and syslog rule
   counts should all grow. In the terminal, press `?` for the same evidence.
2. Watch the warning phase begin around eight seconds. The chart changes after
   each second completes; rolling ratios blend the records in the last five
   seconds, so they do not jump immediately to the new phase's proportions.
   Coverage settles near 80%; read the numeric and conflicting-field hints.
   Unknown is deliberately different from malformed here.
3. Around 16 seconds, inspect **Recent errors**. Freeze the display, read a
   text, JSON, logfmt, colored, or syslog timeout preview, and then resume.
   Ingestion and the demo's cycle continue during the freeze; resuming shows
   current state, not a replay.
4. During the quiet phase, watch **Last record** age. With the five-second window,
   rolling counts eventually become empty and coverage becomes unavailable.
   An empty ratio is not a successful-recovery signal.
5. When info records resume, the last-record age drops, while last-error age
   keeps increasing. Error history and previews still contain the earlier
   incident; those are intentionally different time scopes.
6. Leave the browser running past 60 seconds to see the comparison between its
   last 30 completed seconds and the preceding 30. These intervals can cross
   demo-cycle boundaries. Restarting the demo resets all session history.

The five-second window is a walkthrough setting, not a changed default. Without
`--window 5`, Tell-Tail uses 60 seconds and the rolling display blends multiple
demo phases. Warning/error histories always retain up to 60 completed seconds.
See [the monitoring guide](MONITORING.md) for exact definitions and controls.

## Try parser flags without a log file

Add an existing parser flag to select matching synthetic input:

```sh
tell-tail --demo --profile pino --window 5 --output web --open
tell-tail --demo --level-field priority --window 5 --output web --open
tell-tail --demo --level-path /log/level --window 5 --output web --open
tell-tail --demo --envelope docker --window 5 --output web --open
tell-tail --demo --envelope cri --profile pino --window 5 --output web --open
```

The other profiles work the same way:

| Flag | Generated field | Info / warn / error values |
| --- | --- | --- |
| `--profile pino` | `level` | `30` / `40` / `50` |
| `--profile python` | `levelno` | `20` / `30` / `40` |
| `--profile otel` | `severityNumber` | `9` / `13` / `17` |
| `--profile serilog` | `@l` | `Information` / `Warning` / `Error` |
| `--level-field priority` | Literal `priority` | `info` / `warn` / `error` |
| `--level-path /log/level` | Nested `log.level` | `info` / `warn` / `error` |

Stop the current process with Ctrl+C before trying another command. Inspect
**Recent errors** during seconds 16–24 to see the selected raw sample shape.
Profile and field/path overrides combine as they do for real input. For example,
`--demo --profile python --level-path /event/priority` generates numeric Python
levels inside `event.priority`. Choose either `--level-field` or `--level-path`;
they cannot be combined. The [format reference](LOG-FORMATS.md#nested-json-fields)
explains literal dotted keys, escaped path components, and path limits.

These options select the
**generated examples**, not just the parser: a profile/field/path demo emits only
matching JSON and omits the default demo's deliberate ambiguous records. It has
100% recognition at the default record-size limit, with 80% info / 20% warn in
the warning phase and 30% info / 20% warn / 50% error in the error phase. This does
not claim that applying one profile fixes an arbitrary mixed real-world stream.

An envelope alone wraps the default mixed-format examples, preserving their
unknowns and hints. Combined with a selector, it wraps the selected JSON samples.
All samples pass through normal framing, envelope decoding, classification, and
statistics. Forty examples are prepared once and reused; no replay queue grows.

## Move from the demo to your logs

Keep the parser flags you need and replace `--demo` with a file or pipe. Run
these commands with Tell-Tail 0.3.0 or later, replacing the example app/file names:

```sh
# Live Pino output: the producer supplies the numeric JSON records.
node app.js 2>&1 | tell-tail --profile pino --output web --open

# Read existing records, then follow new ones in a file with nested severity.
tell-tail app.log --from start --level-path /log/level --output web --open

# A saved Docker capture with Pino JSON inside each envelope.
tell-tail saved-docker.ndjson --from start --envelope docker --profile pino --output web --open
```

Without `--from start`, file following begins at the end. Do not add `--demo`
to a real-input command: the demo is a separate input source. On real input,
parser flags configure interpretation only; they never rewrite your logs or
select a matching producer. Restart Tell-Tail to change flags. Unknowns may
remain when records do not match the selected convention.

## What the demo does not exercise

The default stream illustrates recognition and ambiguity with valid records.
It does not generate malformed JSON, oversized records at the default limit,
fatal/trace/debug events, file rotation, or partial container chains. CRI samples
are complete `F` records. Serilog samples use explicit `@l` values rather than
the omitted-info fallback. Those cases have separate
[regression checks](TESTING-DASHBOARDS.md#automated-checks) and
[format rules](LOG-FORMATS.md).

Timestamps inside synthetic records are fixed example values. Rates, ages,
histories, and phase timing use processing time, not those timestamps. The
dashboard shows aggregates and bounded recent error/fatal previews, not the
entire raw stream; intentional unknowns are explained by the hints and examples
above. Nothing is forwarded or persisted unless you explicitly configure an
output or alert destination that does so.

## Other outputs and option combinations

```sh
# Periodic aggregate snapshots, one per second by default
tell-tail --demo --output json

# Plain-text aggregate reports
tell-tail --demo --output text --interval-ms 500
```

These outputs contain aggregates, not the raw synthetic lines. The new activity
and severity-history fields are available in web frames, not CLI JSON schema 2.
The demo reads no stdin. Closing stdin therefore does not finish it; stop it with
`q` in the terminal dashboard, SIGINT/Ctrl+C, or SIGTERM. `q` normally exits 0,
SIGINT exits 130, and SIGTERM exits 143, subject to the normal output-error rules.

| Option | With `--demo` |
| --- | --- |
| `--window`, `--output`, `--interval-ms` | Supported under their normal output-specific rules |
| `--open`, `--web-port` | Supported with `--output web` |
| `--metrics-file` | Supported with `--output prometheus`; writes the requested file |
| `--max-record-bytes` | Supported; an artificially small limit can classify demo records as oversized |
| Alert options | Supported; configured endpoints receive real synthetic breach/recovery notifications |
| A file argument, including `-` | Rejected: the demo is its own input source |
| `--apple-unified` | Rejected: mutually exclusive input source |
| `--once`, `--from` | Rejected: these are file-input options |
| `--format` | Rejected; the default demo mixes formats automatically |
| `--profile`, `--level-field`, `--level-path` | Generate matching JSON samples; normal parser validation still applies |
| `--envelope docker` or `--envelope cri` | Wrap the generated samples before normal parsing; CRI examples are complete `F` records |

For Prometheus stdout, the normal behavior is one final exposition at shutdown;
use `--metrics-file` for periodic snapshots. Alert thresholds, minimum records,
and cooldowns still apply. The default 300-second breach cooldown is longer than
the demo cycle, so configuring alerts does not imply one breach per cycle.
The low-level [webhook demo](../scripts/demo_webhooks.py) is available for a
finite demonstration using a temporary local receiver.

## When the demo looks unexpected

| Symptom | Explanation or next step |
| --- | --- |
| `unexpected argument '--demo'` | Upgrade to 0.3.0 or later and check the binary on `PATH`; 0.2.0 lacks the flag |
| Text reports instead of a full terminal view | Automatic output requires terminal stdout and a controlling terminal; use web mode when redirected |
| No chart immediately | Histories only include completed seconds |
| No records for a few seconds | The 24–30 second phase is intentionally silent; check the elapsed time |
| Error ratio persists during recovery | Older errors remain in the rolling window until they expire |
| Coverage drops to about 80% | Default warning/error phases deliberately include numeric and conflicting-field unknowns |
| Hints persist after coverage returns to 100% | Hints count the session; coverage uses the rolling window |
| `--demo --profile pino` shows 100% coverage | The flag selects matching generated JSON, not the same mixed stream with a different parser |
| `--demo --format json` is rejected | Use the mixed automatic demo, or a profile/field/path flag for a matching JSON demo |
| Coverage is unavailable during silence | An empty window has no denominator |
| Error previews remain after recovery | They are the latest retained error/fatal examples, independent of rolling expiry |
| Last-error age is not “none observed” in the next healthy phase | Activity remembers the whole session, including previous cycles |
| Nothing changes after freezing | Resume; the captured ages, histories, and counts are intentionally held |
| Closing the browser does not stop the demo | Stop the monitor in the launching terminal |

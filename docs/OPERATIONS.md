# Operating Tell-Tail

Tell-Tail keeps bounded in-memory statistics for one source. Terminal and web
dashboards also retain the latest 20 error/fatal previews, capped at 512 UTF-8
bytes each; other record contents are discarded after classification. Restarting
starts a new session; it does not resume from a checkpoint or recover prior
counters or previews. Use a log store independently when retention is required.

Start with [Understanding a live log stream](MONITORING.md) for the meanings of
coverage, severity history, and activity ages. The [built-in demo](DEMO.md) gives
a predictable first session without external logs. Those features are on current
main and are not included in the published 0.2.0 packages.

## Apple unified log input

On macOS, `tell-tail --apple-unified --output web --open` starts native live
capture without a shell pipeline or `jq`. Tell-Tail owns and reaps the capture
process on normal shutdown; a capture exit is a runtime failure. Saved NDJSON
captures use `--format apple-unified` on macOS or Linux. See the
[source and parsing contract](APPLE-UNIFIED.md).

## Tell-Tail's own diagnostics

Runtime errors, capture failures, webhook delivery failures, and web startup/opening
messages go to **stderr**, separate from stdout metrics. For a web session, capture
them with `tell-tail --apple-unified --output web --open 2>tell-tail.stderr.log`.
There is no persistent internal log file or configurable log level. Runtime stderr
writes have a 250 ms budget so blocked diagnostic output cannot hold up shutdown.
Argument validation errors go directly to stderr before startup. Web bind errors
identify the loopback address and port; frame encoding errors identify the
publication sequence and elapsed seconds alongside the cause.

The dashboard writes `[tell-tail]` diagnostics to the browser developer console:
validation and publisher/input failures are errors; disconnections, stale streams,
snapshot HTTP/network failures, and stream capacity are warnings. Repeated stream
and snapshot warnings are suppressed until new live metrics arrive; recovery is
logged once. Error context includes the endpoint, last accepted sequence, and
elapsed seconds. Invalid frames identify the failing field and constraint in both
the console and status banner. Raw frames, source names, and log contents are not
included in these diagnostics. Failed input and publisher errors point to terminal
stderr for the underlying cause. No browser diagnostics are sent to a server.

If the dashboard rejects a frame, save its field-specific console error and the
terminal diagnostic output. The previous instruction to always update the monitor
was misleading: validation failures can also indicate a dashboard bug. A prior
bug rejected valid `window.observed_seconds` values just above 60 after the rolling
window filled; the 100 ms bucket contract now permits that partial edge.

## Linux service example

[examples/tell-tail.service](../examples/tell-tail.service) follows a single file
and publishes `/var/lib/tell-tail/tell-tail.prom`. Adapt its binary and input
paths, create the `tell-tail` account, and grant that account read/traverse access
to the application's log file and parent directories, including newly rotated
files. The service's state directory is writable under its filesystem sandbox.
The example follows new records from EOF; add `--from start` only when replaying
the existing file on every restart is intended.

Configure a textfile collector to read `/var/lib/tell-tail`. New metrics files
are mode 0644, and replacements preserve existing permissions. This example
starts no HTTP server and does not install or configure a collector. Adapt the
sandbox if your input is under a protected home directory.

The service retries missing input paths internally. Runtime failures trigger a
bounded restart policy; argument errors do not restart. SIGTERM normally produces
a final snapshot with `tell_tail_running 0`. The systemd timeout is a supervisor
limit, not a guarantee about uninterruptible filesystem operations. Check the
journal for runtime errors and the metric timestamp for freshness.

## Diagnosing unexpected results

| Symptom | Check |
| --- | --- |
| `--demo` is an unexpected argument | Build current main with `cargo build --locked` and run `./target/debug/tell-tail --demo`. Published 0.2.0 packages lack the new features; the checkout still uses that version number, so `--version` alone cannot distinguish them. |
| Following an existing file shows no records | Following starts at EOF. Append a complete new line, or restart with `--from start` to include existing contents. Use `--once` for a finite text/JSON scan. |
| Bytes arrive but activity still says none observed | Check pending bytes: a partial line is not yet a record. Ensure the producer flushes and terminates records with newlines. A nonempty unterminated line counts at finite EOF; stopping with a signal does not complete it. Apple stream metadata is skipped. |
| Coverage is unavailable | The rolling window contains no records, including after idle expiry. Session totals and last-record age can remain nonzero. This differs from 0% coverage, where records exist but none have recognized severity. |
| High unknown count | Current main recognizes common JSON fields, logfmt, leading text levels/colors, and syslog priority headers. Use `--level-field` for a literal key, `--level-path` for nested JSON, or `--profile` for numeric conventions. Inspect parser hints and the [format reference](LOG-FORMATS.md). Multiline grouping and arbitrary layouts remain unsupported. Unknowns stay in ratio denominators. |
| Conflicting levels hint | Automatic candidate fields disagree. Select the authoritative key with `--level-field`, or use `--level-path` for nested JSON. The hint counts the session, while coverage is rolling. |
| Numeric levels hint | Choose the producer's convention with `--profile pino`, `python`, or `otel`. The same number can mean different levels; custom values remain unknown. |
| Container records stay unknown | Prefer piped `docker logs` or `kubectl logs`. For saved raw captures, select `--envelope docker` or `cri`. CRI partial chains and multiline Docker payloads intentionally remain unknown; no event reconstruction is performed. |
| Apple Default/Fault messages show as unknown | Use `--apple-unified` for native capture or `--format apple-unified` for saved NDJSON. Generic JSON parsing does not apply Apple's level mapping. |
| Apple capture ends unexpectedly | Check the bounded stderr diagnostic from `/usr/bin/log`. Capture runs with your account's permissions and is not silently restarted or reported as normal completion. |
| Parse failures | Malformed structured input, invalid UTF-8 in JSON/logfmt, duplicate selected keys, or unpaired Unicode escapes count once as unknown plus a parse failure. Unrecognized plain text is unknown without a parse failure. |
| Oversized records | Check `--max-record-bytes` (default 1 MiB, maximum 16 MiB). An oversized physical line is drained and counted once as unknown; increasing the limit increases retained-buffer capacity. |
| Coverage shows 100% but unknown count is nonzero | Percentages are rounded. Inspect the exact recognized/total counts and unknown breakdown. Also compare the same time scope: coverage is rolling; the severity distribution and diagnostic totals are session-wide. |
| No severity chart points yet | Wait for the first completed second. Charts omit the current partial second. The browser comparison needs 60 completed seconds even with `--window 5`. |
| Charts cover a different duration than the cards | Histories cover at most 60 completed seconds; rolling cards follow `--window`. A narrow terminal shows only the newest points that fit. Neither uses timestamps inside the logs. |
| Error ratio rises while errors/s stays flat | Total volume may have fallen. Ratios use all records as the denominator; compare total and error counts before concluding that more errors are arriving. |
| Last error is old but its preview remains | Previews are the newest bounded sample, not the current rolling window. They remain until displaced or the session ends. Read the age and processing timestamp to judge recency. |
| Old metrics timestamp | Check the process, destination permissions, collector path, and `--interval-ms`. A stale file can outlive a crashed process; do not rely only on its last `running` value. |
| `source_up` is zero | The followed path may be missing, or the process may have terminated. Check `running` and the timestamp together. A missing source is retried. |
| Growing backlog | The current file is arriving faster than it is being read. Check storage/CPU pressure and record mix. Backlog is unavailable for stdin and does not measure unread data in other rotated generations. |
| Output error/status 1 | For stdout, a full report/frame could not complete within two seconds, or an I/O operation failed. Drain stdout continuously. Cancellation gives output a shorter grace period. Broken downstream pipes exit quietly. |
| Counts differ after rotation | Use rename-based rotation with completed lines. Late writes after handoff and truncate/regrow cycles between observations can be missed. Partial records at observed generation boundaries are discarded and counted separately. |
| Web process stays running after stdin EOF | This is intentional: final metrics remain available until Ctrl+C. `--once --output web` is rejected. |
| Web stream receives 429 | Four SSE streams are already active. Close another stream; current metrics remain available through `/api/snapshot`. |
| Dashboard shows stream capacity | Close another live tab or SSE client, then use Retry connection. Capacity does not automatically retry. |
| Dashboard shows stale/reconnecting | Last values remain visible, including when frozen. Check that the local monitor is running; retries back off to eight seconds. A disconnect alone does not indicate completion. |
| Activity ages stop increasing | Ages belong to the displayed snapshot. Freeze, stale/disconnected data, and final state intentionally preserve them. Check the display and connection status; a quiet but live input continues publishing increasing ages. |
| Browser space bar does not freeze | Tab to **Freeze display** and press Space or Enter, or click it. Only the terminal dashboard has a global space shortcut. |
| Dashboard display does not advance | Check Freeze display / Resume display. Hidden tabs disconnect deliberately and reconnect when shown; freezing the display does not pause ingestion or alerts. |
| `--open` cannot launch a browser | Use the printed loopback URL manually. The desktop opener is optional and never owns server shutdown; no Node runtime is needed. |
| Web page receives 403 | Use the exact printed `127.0.0.1` URL and port; foreign Host and Origin values are rejected. |
| Web stream disconnects while ingestion continues | A socket with no write progress for two seconds is closed independently. Drain the stream or reconnect for current state. |
| Ratios differ from request failure rates | Ratios describe physical log lines, including unknowns. Error ratio combines error and fatal. A request can emit many lines or none. |

## Compatibility and validation

CLI JSON is NDJSON with schema 2; [web JSON](WEB-FRAMES.md) has a separate schema 1
with decimal counter strings. See [web server behavior](WEB-SERVER.md) for local
access and shutdown limits. Empty ratios are `null`; Prometheus window ratios
are `NaN` when empty. Prometheus counters reset on restart, and the set of severity
labels is fixed. Window membership uses processing time with 100 ms buckets;
replaying an old file measures present scan throughput. See the
[migration notes](../CHANGELOG.md), [output isolation contract](OUTPUT.md), and
[validation commands](TESTING-0.2.md).

The compact dashboard shows unknown and parse-failure counts when at least
20 columns and 8 rows are available. Full layout requires 60 × 18, and recent
error previews require 60 × 20. Longer activity/help text can be clipped at small
sizes; enlarge the terminal for a complete view. Smaller terminals remain usable
for quitting, but cannot display all diagnostics. See the
[controls and display guide](MONITORING.md#reviewing-and-controlling-the-display).

# Operating Tell-Tail

Tell-Tail keeps bounded in-memory statistics for one source. It discards log
contents after classification. Restarting starts a new session; it does not resume
from a checkpoint or recover prior counters. Use a log store independently when
retention is required.

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
| High unknown count | Check `--format` and `--level-field`. JSON severity must be a top-level string. ANSI prefixes, logfmt, multiline grouping, nested fields, and numeric levels are unsupported. Unknowns remain in ratio denominators. |
| Apple Default/Fault messages show as unknown | Use `--apple-unified` for native capture or `--format apple-unified` for saved NDJSON. Generic JSON parsing does not apply Apple's level mapping. |
| Apple capture ends unexpectedly | Check the bounded stderr diagnostic from `/usr/bin/log`. Capture runs with your account's permissions and is not silently restarted or reported as normal completion. |
| Parse failures | Malformed JSON, invalid UTF-8 in JSON, duplicate severity fields, or unpaired Unicode escapes are counted once as unknown plus a parse failure. Plain unrecognized text is unknown without a parse failure. |
| Oversized records | Check `--max-record-bytes` (default 1 MiB, maximum 16 MiB). An oversized physical line is drained and counted once as unknown; increasing the limit increases retained-buffer capacity. |
| Old metrics timestamp | Check the process, destination permissions, collector path, and `--interval-ms`. A stale file can outlive a crashed process; do not rely only on its last `running` value. |
| `source_up` is zero | The followed path may be missing, or the process may have terminated. Check `running` and the timestamp together. A missing source is retried. |
| Growing backlog | The current file is arriving faster than it is being read. Check storage/CPU pressure and record mix. Backlog is unavailable for stdin and does not measure unread data in other rotated generations. |
| Output error/status 1 | For stdout, a full report/frame could not complete within two seconds, or an I/O operation failed. Drain stdout continuously. Cancellation gives output a shorter grace period. Broken downstream pipes exit quietly. |
| Counts differ after rotation | Use rename-based rotation with completed lines. Late writes after handoff and truncate/regrow cycles between observations can be missed. Partial records at observed generation boundaries are discarded and counted separately. |
| Web process stays running after stdin EOF | This is intentional: final metrics remain available until Ctrl+C. `--once --output web` is rejected. |
| Web stream receives 429 | Four SSE streams are already active. Close another stream; current metrics remain available through `/api/snapshot`. |
| Dashboard shows stream capacity | Close another live tab or SSE client, then use Retry connection. Capacity does not automatically retry. |
| Dashboard shows stale/reconnecting | Last values remain visible, including when frozen. Check that the local monitor is running; retries back off to eight seconds. A disconnect alone does not indicate completion. |
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
20 columns and 8 rows are available. Full layout requires 60 × 22. Smaller
terminals remain usable for quitting, but cannot display all diagnostics.

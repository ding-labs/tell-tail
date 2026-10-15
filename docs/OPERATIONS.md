# Operating Tell-Tail

Tell-Tail keeps bounded in-memory statistics for one source. It discards log
contents after classification. Restarting starts a new session; it does not resume
from a checkpoint or recover prior counters. Use a log store independently when
retention is required.

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
| Parse failures | Malformed JSON, invalid UTF-8 in JSON, duplicate severity fields, or unpaired Unicode escapes are counted once as unknown plus a parse failure. Plain unrecognized text is unknown without a parse failure. |
| Oversized records | Check `--max-record-bytes` (default 1 MiB, maximum 16 MiB). An oversized physical line is drained and counted once as unknown; increasing the limit increases retained-buffer capacity. |
| Old metrics timestamp | Check the process, destination permissions, collector path, and `--interval-ms`. A stale file can outlive a crashed process; do not rely only on its last `running` value. |
| `source_up` is zero | The followed path may be missing, or the process may have terminated. Check `running` and the timestamp together. A missing source is retried. |
| Growing backlog | The current file is arriving faster than it is being read. Check storage/CPU pressure and record mix. Backlog is unavailable for stdin and does not measure unread data in other rotated generations. |
| Output error/status 1 | A full report/frame could not complete within two seconds, or an I/O operation failed. Drain stdout continuously. Cancellation gives output a shorter grace period. Broken downstream pipes exit quietly. |
| Counts differ after rotation | Use rename-based rotation with completed lines. Late writes after handoff and truncate/regrow cycles between observations can be missed. Partial records at observed generation boundaries are discarded and counted separately. |
| Ratios differ from request failure rates | Ratios describe physical log lines, including unknowns. Error ratio combines error and fatal. A request can emit many lines or none. |

## Compatibility and validation

JSON is NDJSON with schema 2. Empty ratios are `null`; Prometheus window ratios
are `NaN` when empty. Prometheus counters reset on restart, and the set of severity
labels is fixed. Window membership uses processing time with 100 ms buckets;
replaying an old file measures present scan throughput. See the
[migration notes](../CHANGELOG.md), [output isolation contract](OUTPUT.md), and
[validation commands](TESTING-0.2.md).

The compact dashboard shows unknown and parse-failure counts when at least
20 columns and 8 rows are available. Full layout requires 60 × 22. Smaller
terminals remain usable for quitting, but cannot display all diagnostics.

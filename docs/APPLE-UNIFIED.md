# Apple unified log input

On macOS, monitor live system activity with one command:

```sh
tell-tail --apple-unified --output web --open
```

For the terminal dashboard use `tell-tail --apple-unified`; JSON/text/Prometheus
outputs and alerts also accept this source. Traffic follows actual system
activity. No `jq`, shell pipeline, saved raw log, or extra dependency is required.

## Source and lifecycle

`--apple-unified` starts `/usr/bin/log stream --style ndjson --level info --type log`
directly, without a shell. The executable path and arguments are fixed. Capture
runs with the current account's permissions, uses one owned child process, and
selects Apple parsing automatically. The source label is `macOS unified log`;
backlog is unavailable. Stdin is not a data source, including when it is a
terminal or EOF; the terminal dashboard still accepts its normal keyboard input.

The source cannot be combined with a file argument, `--once`, `--from`, or a
different explicit format. Live capture is macOS-only; other platforms reject
it with an argument error. The web feature remains optional: native capture also
works in a CLI-only build.

Ctrl+C, SIGTERM, terminal quit, or an output/startup failure stops and reaps the
owned capture process. Private stdout/stderr pipes are nonblocking; idle input
polls for at most 100 ms before checking cancellation. No capture descriptor is
used for terminal keyboard input, and inherited stdin/stdout flags are unchanged.
Capture stderr is drained with bounded work and at most 4 KiB retained for failure
diagnostics. A full stderr pipe cannot indefinitely block ingestion.

Any unexpected capture exit, including status zero, or premature stdout closure
produces state `failed` and exit 1, with the bounded native diagnostic on stderr.
It is not normal EOF and does not retain a misleading completed web session.
There is no automatic restart that could conceal a capture gap. Normal monitor
signals retain the existing 130/143 exit codes and attempt the final stopped frame.

## Message types and metadata

The selected top-level field is `messageType`, independent of `--level-field`:

| Apple value | Tell-Tail severity |
| --- | --- |
| Default | info |
| Info | info |
| Debug | debug |
| Error | error |
| Fault | fatal |

Value matching is case-insensitive and supports JSON escapes. Unknown non-null
values, including non-string types, count as unknown. Generic `auto`/`json`/`text`
parsing and their level aliases are unchanged; Apple mapping is opt-in.

The format ignores non-object stream text (including the native banner), empty
lines, and valid objects with missing/null `messageType`, such as tool summaries.
Those bytes, including line terminators, are counted as skipped bytes and do not
affect entry/severity counts or ratio denominators. A malformed JSON object,
duplicate `messageType`, invalid UTF-8, or invalid Unicode escape counts as one
unknown record plus a parse failure. Oversized lines retain the normal one
unknown/oversized-record behavior because their contents are not retained.

Byte throughput measures original capture output, including metadata, without
rewriting JSON or adding a synthetic `level` field. Entry rates and ratios use
only classified records, including unknown/malformed/oversized records. Error
ratio combines error and fatal, as in the other formats. Native info-level capture
normally excludes Debug events; Debug mapping also applies to saved captures.

## Saved or externally streamed captures

The explicit format works with files and stdin on macOS and Linux:

```sh
# Scan a finite exported NDJSON capture.
tell-tail capture.ndjson --format apple-unified --once

# Follow an NDJSON capture that another process is appending to.
tell-tail capture.ndjson --format apple-unified --from start --output web --open

# Keep custom native log filters, while Tell-Tail handles parsing.
/usr/bin/log stream --style ndjson --level debug --type log |
  tell-tail --format apple-unified --output web --open
```

Use newline-delimited JSON exported with `log --style ndjson`. Binary `.tracev3`
files, `.logarchive` directories, and the multiline array from `--style json`
are not this input format. For a file scan `--once` exits at EOF; for web output,
use a followed file or stdin as described in the [dashboard guide](WEB-DASHBOARD.md).

## Validation

Parser/engine tests cover all Apple levels, escaped and nested fields, null/missing
metadata, unknown values, duplicate keys, malformed JSON, surrogate/UTF-8 errors,
large ignored numbers, oversized records, physical skipped-byte accounting,
arbitrary chunk boundaries, and saved-file CLI output. Adapter tests exercise
stderr flooding, nonzero/zero capture exit, closed stdout, idle cancellation,
bounded diagnostics, and process reaping without requiring macOS logging.

macOS integration tests use real live events to verify source labeling, stdin EOF
independence, native web snapshots, SIGINT/SIGTERM cleanup, and failed capture
propagation. A local pseudo-terminal smoke also verifies launching web capture
with terminal stdin. Generic format/output compatibility remains covered by the
existing fixtures and tests.

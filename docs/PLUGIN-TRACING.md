# Tool-call tracing

Available in this checkout; not a claim about published packages. Build with the
default `web` feature to receive **Codex OTLP HTTP/protobuf tool results**. The
passive `tell-tail mcp --open` command still requires no client configuration.

The Tool calls tab shows a bounded table with tool identity, outcome, duration,
execution label, filters, pagination, and an inspector. Pause freezes the displayed
call revisions, including details and filtered queries, while intake continues.
Expired snapshots require an explicit refresh. Copy includes diagnostic metadata,
never arguments, outputs, credentials, or raw execution IDs.

## Verified scope

On Codex Desktop 26.1111.21308 (engine 0.169.0-alpha.11.1), Railway documentation
calls made sequentially and in parallel inside `functions.exec` export separate
results. The wrapper is also a separate tool call. Source namespace
`mcp__codex_apps__railway` is mapped explicitly to `railway`; other namespaces are
retained in the qualified tool name and remain **unidentified plugins**. Search
can match those namespaces without inventing a plugin mapping.

This is result-only capture. Starts, cancellations, subagent coverage, parent
relationships, and provider-level errors are unverified. A source `success=true`
means reported tool execution success; it is not proof that a business operation
succeeded. A shell command exiting nonzero can still have execution success.
Missing source completion timestamps stay missing; the table labels receipt time
when displaying it instead. Duration is client-reported, with approval/queue scope
unverified. No account or balance access is needed for verification.

See the [capture audit](PLUGIN-TRACE-CAPTURE-AUDIT.md) for evidence and limitations.

## Set up the local exporter

Use a private directory and a stable loopback port:

```sh
mkdir -p "$HOME/.config/tell-tail"
chmod 700 "$HOME/.config/tell-tail"
cargo run --locked -- mcp --trace-port 4318 \
  --trace-token-file "$HOME/.config/tell-tail/trace.key" \
  --trace-config-out "$HOME/.config/tell-tail/otel.toml" --open
```

The receiver creates a random credential file with mode 0600 if it does not exist.
It writes a new mode-0600 exporter fragment and refuses to overwrite an existing
fragment. Neither option edits your client configuration. Keep both files private;
do not paste their contents into issues, chats, URLs, or command arguments.

1. Back up `~/.codex/config.toml` privately.
2. Review the generated fragment and merge its `[otel]` settings into the user
   configuration. Do not append a second `[otel]` table. Preserve an existing or
   managed exporter; where required, use an approved collector routing setup.
3. Restart the desktop app so the relevant execution engine loads the exporter.
4. Invoke a harmless plugin lookup. Check **Receiving tool results**, then filter
   Plugin to `railway` and inspect the call. An unrelated telemetry batch alone
   does not prove plugin coverage.

The generated fragment disables prompt and agent-response logging. The exporter
may still send tool arguments/output in a batch; Tell-Tail discards them before
queueing or retaining normalized observations. Unknown fields are never printed.
The receiver is an in-memory diagnostic tool, not a durable audit system.

On later collector starts, reuse `--trace-port` and `--trace-token-file`, and omit
`--trace-config-out` unless you want a new fragment at a new path. Port `0` chooses
an available port, useful for tests, but the exporter must use that chosen port.
The dashboard runs on a **separate** loopback port printed at startup.

For headless live output, replace `--open` with `--output json`. Tool envelopes
are `call_update`, `trace_source`, and `trace_gap`; ordinary MCP envelopes retain
their meaning. Update by call ID/revision rather than counting revisions as calls.
Identifiers, revision watermarks and cumulative counters serialize as decimal
strings. Collector-local labels change when Tell-Tail restarts.

## Disable and restore

Restore the original `[otel]` settings (or remove only the section you added),
then restart the client to unload the exporter. Stop Tell-Tail with Ctrl+C after
that restart. Restoring the file alone does not unload an exporter already held
in memory by the current app process. Delete the private credential/fragment
when you no longer need this configuration. Never leave an active exporter
pointing at a stopped temporary receiver.

## Limits and failure behavior

- Separate authenticated loopback intake; exact Host, no browser Origin, no
  compression/chunked transfer; HTTP/protobuf only at `POST /v1/logs`.
- 1 MiB requests, 1,024 records per batch, 32,768 inspected protobuf fields,
  four simultaneous connections, and a two-second total connection deadline.
- At most four queued batches and 1,024 queued observations, whichever is reached
  first. Saturation returns 503; invalid/oversized requests get explicit rejection.
- 2,000 current calls, 8,000 revisions, bounded deduplication/source indexes, and
  a 16 MiB conservative store charge. Eviction can expire paused snapshots.
- Browser pages contain at most 100 calls / 64 KiB. SSE publishes coalesced source
  status/watermarks at up to five changes per second, with bounded stream slots.
- Source counters expose rejected batches/records and ignored non-tool events.
  `quiet` means no recent telemetry; it does not prove a client disconnect or
  a failed service. Exporter batching and rejection can cause delay or gaps.

No-default-features builds retain passive and finite replay modes and reject
network intake explicitly. Synthetic fixtures use no client logs/settings:

```sh
cargo run --locked -- mcp --trace-replay \
  tests/fixtures/tool-trace/lifecycle.ndjson --output json --once
```

For an offline browser demo using that same fixture, use `--open` instead of
`--output json --once`. The page explicitly labels imported synthetic evidence;
no listener or client-log collector is started for this replay.

The [implementation plan](PLAN-PLUGIN-TRACING.md) records the intended contracts.
The two 24-hour soak scenarios remain optional, non-blocking background checks.

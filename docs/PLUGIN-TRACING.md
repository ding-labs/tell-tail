# Tool-call tracing

Available in this checkout; not a claim about published packages. Build with the
default `web` feature to receive **Codex OTLP HTTP/protobuf tool results**. The
passive `tell-tail mcp --open` command still requires no client configuration.

The [Activity workspace](DESKTOP-ACTIVITY.md) is the default for new managed
collectors. Its Tools & MCP destination combines results with server diagnostics.
The existing MCP dashboard remains at `/mcp`. The same installed collector supports
[user-selected application logs](LOG-SOURCES.md), with saved settings and original
log metrics/error review in Logs. Those previews are separate from metadata-only
tool tracing.

The MCP dashboard’s Tool calls tab shows a bounded table with tool identity, outcome, duration,
execution label, filters, pagination, and an inspector. Pause freezes the displayed
call revisions, including details and filtered queries, while intake continues.
Expired snapshots require an explicit refresh. Copy includes diagnostic metadata,
never arguments, outputs, credentials, or raw execution IDs.

## Verified scope

On Codex Desktop 26.1111.21308 (engine 0.169.0-alpha.11.1), Railway documentation
calls made sequentially and in parallel inside `functions.exec` export separate
results. The wrapper is also a separate tool call. A subsequent live check verified
GitHub and Railway calls made together, with separate rows and durations.

There is no provider allowlist. Every hosted namespace matching
`mcp__codex_apps__<provider>` supplies its complete `<provider>` suffix as the
plugin filter label, including internal underscores. Suggestions automatically
populate from retained calls (up to 128 distinct labels). These labels identify
**observed source namespaces**, not installed manifest IDs or friendly names.
Other tools retain their qualified name and transport server and remain searchable.
New hosted providers need no Tell-Tail configuration.

A cached bundle is not evidence of an enabled plugin. The installed-plugin CLI
initially timed out when querying all marketplaces; a later scoped query for
`tell-tail-local` succeeded. Capture does not depend on catalog polling.
Installed-but-unused plugins and skills without tool calls do not produce rows.
This is automatic discovery of observed activity, not an installed-plugin inventory.

This is result-only capture. Starts, cancellations, subagent coverage, parent
relationships, and provider-level errors are unverified. A source `success=true`
means reported tool execution success; it is not proof that a business operation
succeeded. A shell command exiting nonzero can still have execution success.
Missing source completion timestamps stay missing; the table labels receipt time
when displaying it instead. Duration is client-reported, with approval/queue scope
unverified. No account or balance access is needed for verification.

See the [capture audit](PLUGIN-TRACE-CAPTURE-AUDIT.md) for evidence and limitations.

## Local call inspection

On macOS, use `tell-tail tracing enable --inspect-calls` for a new installation.
For an existing installation, run the **new executable** with
`tracing upgrade --inspect-calls`. The upgrade restarts the collector, which clears
in-memory history. A subsequent upgrade preserves the setting. Use
`tracing upgrade --inspect-calls=false` to return to metadata-only capture.
For a standalone web receiver, pass `--inspect-chat-index PATH/TO/session_index.jsonl`.

The collector matches the result's conversation ID exactly against the most
recent 512 KiB of the local chat-name index. The displayed title is the name at
capture time. For recognized shell and scripting tools it can retain up to
1,536 UTF-8 bytes of submitted command or script in memory. A command may itself
contain private paths or secrets. The local inspector fetches that text when
expanded. Lists expose the operation label and, when matched, chat title.
Other recognized tools can show a file basename, terminal polling, a browser action
title, web operations or recognized parameter names. The shell working directory
is shown by its basename. **Copy diagnostic summary** omits the inspection fields. Tool output bodies and
complete conversation histories are never stored by this mode. Unsupported
inputs, absent names and unavailable process exit codes remain unknown.

## Persistent macOS setup

Build this checkout with the default `web` feature, then:

```sh
cargo run --release --locked -- tracing enable
cargo run --release --locked -- tracing status
```

After installation, the self-contained management executable is:

```sh
"$HOME/Library/Application Support/Tell-Tail/tell-tail" tracing status
```

Setup copies the executable into that private application-support directory,
creates a private credential, backs up the original user config, and writes only
an owned `[otel]` table. It refuses another exporter or user edits to its table.
`CODEX_HOME` selects the configuration home at installation; otherwise it uses
`~/.codex`. Setup never changes managed/workspace configuration layers, which may
still override user settings; verify actual receipt after setup.

The per-user LaunchAgent `ing.ding.tell-tail.tracing` starts at login, stays
independent of app/chats, and restarts after a collector exit. Default ports are
63701 for authenticated intake and 61193 for the dashboard. Both are loopback
only. `--trace-port` and `--web-port` accept distinct stable nonzero ports; port
conflicts fail without changing Codex settings. No unbounded service logs are
written. `tracing status` reports exporter ownership, service loading and receiver
health without printing credentials. Repeating enable with unchanged settings is
idempotent; disable before changing ports. Use the upgrade command below to
replace the copied executable while retaining the credential and ports.

Restart Codex **once** to load the exporter, unless that exact endpoint and
credential are already active. Later app restarts keep tracing enabled. A collector
restart resets its bounded in-memory history; an app restart alone does not stop
the collector. This does not provide durable audit storage.

For a known temporary receiver migration, stop that receiver and pass
`--import-token-file /absolute/path/to/existing-private-key` to `tracing enable`,
using its existing trace port. Never put the token value in shell arguments.

To disable:

```sh
"$HOME/Library/Application Support/Tell-Tail/tell-tail" tracing disable
```

Then restart Codex to unload the exporter it still holds in memory. Disable
removes only unchanged, owned exporter settings and the LaunchAgent. It retains
private backup/key/installation files for re-enabling. If the exporter was edited,
it refuses to remove it; reconcile those edits before retrying. Never restore the
whole backup over later user changes. An interrupted setup may leave `setup.lock`;
check that setup is no longer running before removing that lock and retrying.

## Upgrade, roll back, or uninstall the collector

Run the **new** executable's `tracing upgrade` command after obtaining a newer
build. A plugin update alone does not replace the separately installed collector.
For a source checkout, use `cargo run --locked -- tracing upgrade`. The command
preserves the credential, ports, exporter and unrelated settings, retains one
previous executable and launch command, migrates owned MCP services to the
Activity workspace, restarts an enabled collector, and checks receiver and activity
health. Edited service definitions are preserved and refused.
A disabled installation stays disabled. Failed activation attempts restore the
previous executable and service automatically; failures of that recovery are
reported explicitly. Collector restarts clear in-memory history; Codex does not
need a restart when the endpoint and credential remain unchanged.

`tracing status` reports the managing executable's version, the installed version
when known, the running collector version when reported, and rollback availability.
For an explicit rollback, run `tracing rollback` using a supported executable.
Rollback swaps the current and previous executables while preserving configuration.

`tracing disable` remains the reversible option that retains private setup files.
For complete collector removal, run `tracing uninstall` before removing the plugin
in Desktop. It removes only owned exporter settings, the LaunchAgent, collector
executables, credentials, metadata and the private setup backup. User-added files
in the directory are preserved. Changed exporter settings block removal rather
than being overwritten. Interrupted final cleanup can be retried. Restart Codex
after uninstall to unload its in-memory exporter; removing the plugin alone does
not stop the collector service.

Native lifecycle verification uses an isolated service label and private temporary
installation, leaving an existing collector alone:

```sh
cargo build --locked --bin tell-tail
TELL_TAIL_TEST_BINARY="$PWD/target/debug/tell-tail" cargo test --lib \
  native_lifecycle_preserves_credentials_recovers_and_uninstalls -- --ignored
```

This test requires macOS with an active GUI launchd domain. It covers active and
disabled upgrades, explicit rollback, failed activation recovery, credential/config
preservation, edited-exporter refusal, uninstall and interrupted cleanup recovery.

## Installable plugin preview

The plugin source is `packaging/codex-plugin`. Build a self-contained native
preview with Python 3.11+ and the repository's Rust toolchain:

```sh
python3 scripts/package-tracing-plugin.py
```

This produces `dist/tell-tail-macos-<architecture>/` and a ZIP of that local
marketplace, including the executable, onboarding skill, dependency notices and
build provenance. Users of the built bundle do not need Python, Node or Rust.
Extract the ZIP and add its directory as a local marketplace in Codex (or
`codex plugin marketplace add /absolute/path/to/extracted-marketplace`), install
Tell-Tail, then run its setup. The source directory alone lacks the executable;
install the generated bundle. A fresh macOS arm64 desktop install, onboarding
from the cached plugin, clean collector setup, app restart, and live GitHub and
Railway capture were [verified](validation/plugin-trace-desktop-install-2026-11-19.json).
Desktop install/setup clicks were user-assisted because Codex blocks automated
control of its own window; catalog, files, service, calls and dashboard were
independently checked. This preview has not been published to the public directory.

The [supported onboarding mechanism](https://developers.openai.com/plugins/build/plugins#add-an-onboarding-skill)
is a skill invoked when users run setup after installation. This package has no
silent lifecycle hooks. Setup installs the service/config described above. Before
removing the plugin, run `tracing disable`: plugin removal alone cannot stop the
separately installed service. Automatic installed-plugin inventory, durable
history, and guaranteed coverage of every Codex execution path remain outside
this preview.

## Manual foreground exporter setup

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
may still send tool arguments/output in a batch. The default receiver discards
these fields. Opt-in local call inspection retains the bounded fields described above. Unknown fields are never printed.
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

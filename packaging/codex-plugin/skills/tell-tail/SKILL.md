---
name: tell-tail
description: Manage Tell-Tail for ChatGPT and Codex. Open local desktop activity, inspect MCP and plugin tool results, or set up, upgrade, roll back and remove capture. Use for Tell-Tail observability, not for changing provider-side application tracing.
---

The packaged executable is `../../bin/tell-tail` relative to this SKILL.md's
directory. Resolve its absolute path before invoking it. This package is for
macOS and the architecture recorded in `../../build.json`; no Python, Node,
Rust toolchain, or remote account is needed on the user's machine. If the
executable is missing or incompatible, report an incomplete/wrong-platform
package rather than downloading and executing an unverified replacement.

For setup, explain that Tell-Tail enables a persistent, authenticated loopback
exporter and a per-user login service. When the user requests setup or enabling
tracing, run the packaged executable with `tracing enable`. It refuses to
replace an existing or edited exporter; preserve those settings if it refuses.
Do not print config.toml, otel.toml, credentials, or private backups.

Run `tracing status` to inspect health. Use its `dashboard` URL to open Activity,
with Tools & MCP, Problems and Sources alongside it. The login service never
opens browser windows automatically. Open the URL when the user requests the
dashboard; do not launch a second collector when the managed service is healthy.
A temporary passive session is `desktop --open`; `desktop --demo --open` reads
only synthetic data. Neither enables the exporter.
The receiver must be healthy and `exporter_configured` and `service_loaded` must
be true. `waiting_for_exporter` after first setup normally requires one Codex
restart. Let the user restart their app; do not terminate active chats. If this
same endpoint and credential were already loaded, another restart is unnecessary.
Future app restarts keep tracing enabled. The service also starts at login and
recovers after a collector exit.

For a live check, use a harmless read-only tool already relevant to the user's
task, then verify its named row through `/api/traces/calls` at the dashboard
origin. Do not query every connected account merely to populate the UI. Receipt
of an unrelated telemetry batch is not proof of that plugin's tool capture.

The desktop adapter recognizes an allowlisted subset of local connection,
plugin, browser/runtime and response-routing events. Sources describes the
observed scope and limitations. Do not promise every ChatGPT desktop/cloud event,
infer chat state from an app-wide record, or treat ordinary closure as failure.

The plugin filter discovers hosted provider namespace labels from retained
tool results. These are not verified installed-plugin manifest IDs. Tools from
other namespaces remain visible by their qualified names. Skills alone have no
tool call to trace. Results depend on what the client exports; starts, parent
relationships, cancellations, and complete coverage of all execution paths are
not guaranteed. Arguments, outputs, credentials and raw chat/call IDs are not
retained. History is bounded and in memory, and resets when the collector restarts.

When the user requests an update, run this packaged executable's `tracing upgrade`.
It replaces the independent collector, preserves credentials and ports, and checks
health. Owned older MCP launch definitions migrate to `desktop`; verify
`collector_command=desktop` and `activity_healthy=true`. The previous executable
and launch command are retained for `tracing rollback`; failed
activation attempts restore it automatically. Report recovery failures without
claiming the collector is healthy. A disabled installation stays disabled. Collector
restart clears in-memory history; it does not require an app restart with unchanged
credentials. Run `tracing status` afterward and verify actual call capture.

When the user requests disabling tracing, run `tracing disable` to retain private
setup files for re-enabling. When they request removal, run `tracing uninstall`
before removing this plugin. Restart Codex afterward to unload its in-memory
exporter. Plugin removal alone does not stop the separately installed login
service. Disable keeps a private credential and backup under
`~/Library/Application Support/Tell-Tail` for re-enabling. Uninstall removes those
owned files; it preserves unrelated config and user-added files. Never
restore the whole backup over newer user edits. No lifecycle hook runs silently.

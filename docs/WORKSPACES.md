# One Tell-Tail, two workspaces

Tell-Tail is the product. Logs and AI Activity are complementary workspaces;
ChatGPT and Codex are integrations. The workspace switch and shared appearance
are available in Tell-Tail 0.5.0 and later.

| Entry point | Opens first | What switching does |
| --- | --- | --- |
| `tell-tail app.log --output web --open` | Logs | AI Activity explains how to open the plugin's collector or start a desktop session |
| `tell-tail --demo --output web --open` | Logs | Keeps the existing log demo, charts and error review |
| `tell-tail desktop --open` | AI Activity | Logs offers desktop aggregate metrics and user-selected application logs |
| Managed plugin collector | AI Activity | Same two workspaces; application log sources are saved across collector restarts |
| `tell-tail mcp --open` | Focused MCP diagnostics | Logs shows aggregate followed-log metrics; the AI Activity link retains this focused MCP session |

The root URL preserves the collector's entry point. `/logs` is the metrics
workspace; `/activity` is AI Activity or its explicit entry page when this
process is only monitoring a file/stdin stream. `/mcp` retains focused diagnostics
in desktop mode. Explicit application-file selection is available through the
protected [source-management API](LOG-SOURCES.md); no directory browsing, automatic
port scanning or extra automatic log discovery is introduced.

Switching pages leaves the collector running. Opening AI Activity in a file
monitor does not reinterpret arbitrary application logs as AI events. The entry
page explains how to use an existing managed collector or open a temporary
`desktop --open` session in a separate local dashboard. This preserves the
original file/stdin input and process lifecycle.

## Logs stays first-class

The original throughput and severity monitor keeps its rolling chart, severity
history, parsing guidance, exact values, source freshness, error previews, freeze
controls, terminal output, Prometheus and HTTP/SSE contract. Header styling and
navigation change; file/stdin counting and parsing do not.

For desktop collectors, complete records from the existing follower also feed
bounded `Stats` counters. The original metrics renderer and frame/SSE contract
are reused. This aggregate stream counts record content bytes, excludes oversized
and unfinished records, and includes bounded imported history when received.
Charts measure collector processing time, not original event time. Both mirrored
physical records count in Logs; AI Activity preserves MCP mirror deduplication.
These differences are visible in the desktop Logs explanation.

Raw desktop bodies are classified transiently and discarded; there is no desktop
recent-error buffer. Unavailable low-level ingestion diagnostics are hidden in
that workspace; use AI Activity → Sources for follower coverage. General
file/stdin Logs continues to offer its existing raw error previews and physical
input-byte diagnostics. Tracing configuration is unchanged.

## Application log sources

Use **Manage sources** in Logs or **Application logs** in AI Activity → Sources to
add, edit, pause, resume or remove a regular local file. These sources retain the
original error previews and diagnostics; automatic desktop capture stays metadata
only. The managed plugin saves source configuration across collector restarts.
See [application log sources](LOG-SOURCES.md) for persistence, parsing options,
limits and the difference between pausing monitoring and freezing the display.

## A shared product frame

Both workspaces use the same Tell-Tail mark, Logs / AI Activity navigation,
colors, typography and appearance control. Light, dark and system appearance
carry between pages on the same dashboard origin. Existing AI Activity
appearance preferences are read for migration; new preferences use
`tell-tail-preferences`. Appearance and row density are stored as browser preferences; the current
application-log selection uses tab session storage. Source configuration lives in
the local collector registry, separately from those preferences. Storage
restrictions still allow appearance changes for the current page.

AI Activity keeps its compact event feed, inspector, time lanes and subnavigation.
Logs keeps the visual structure suited to metrics and log review. The product
frame is shared without forcing these different tasks into the same chart layout.
Browser cache restoration reconnects AI Activity while keeping a paused view fixed.

No beta packaging, release, version bump or changes to plugin installation are
part of this pass. The plugin's descriptive text identifies its integration and
both workspaces without creating a second product identity.

See the [validation record](VALIDATION-WORKSPACES.md) for regression checks,
reviewed screenshots and installed-collector acceptance.

# Passive MCP diagnostics

**Available in this checkout; not in the published 0.2.0 packages.**

Hosted documentation lives on [docs.ding.ing](https://docs.ding.ing/):
[released Tell-Tail guides](https://docs.ding.ing/tell-tail/) and
[unreleased preview](https://docs.ding.ing/tell-tail/next/). Read this guide online
at [MCP diagnostics](https://docs.ding.ing/tell-tail/next/mcp/). The preview uses a
pinned source revision and can lag newer checkout changes. See the
[documentation publication steps](RELEASING.md#hosted-documentation) for that update.

```sh
cargo run --release --locked -- mcp --open
# After installing this checkout with cargo install --path . --locked:
tell-tail mcp --open
```

The command opens an embedded local dashboard and watches existing MCP logs.
It needs no account, model key, plugin, server wrapper, configuration change,
or client restart. Ctrl+C stops the terminal-owned watcher. Closing the page
does not stop collection. A failed browser opener leaves a working printed URL.

Try the same collector and parsers against synthetic files instead:

```sh
tell-tail mcp --demo --open
```

Demo mode reads no client logs. Every two seconds it replays startup, an OAuth
failure, explicit timeout, recovery, protocol metadata, intentional shutdown,
and unexpected disconnect. Claude's aggregate/server mirrors appear as one
observation. Its temporary files reset every 16 seconds and are deleted on normal
shutdown. The dashboard prominently labels the demo.

## What developers can investigate

The first view shows reported failures, their server and execution scope, a live
timeline, source freshness, and recognition limits. Matching affirmative evidence
can mark a group recovered; another execution's success cannot clear it. Repeated
failures increment observation counts, not an invented count of outages.

Use client/server filters to narrow the overview. Timeline search, category, and
time filters query up to 2,000 retained events, with 100 per page. Pause holds the
display while capture continues. Older evidence also pauses updates; Latest
evidence resumes. Inspect shows safe metadata and source/generation/byte-offset
references. Copy diagnostic summary copies that metadata without raw errors,
thread IDs, arguments, or results. Evicted details explicitly report unavailable.

Watcher connection, file freshness, and last observed server state are separate.
Imported records are labeled history. A silent log, a normal close, or an old
ready record does not establish current health. The source panel exposes unknown
formats, ignored lines, skipped history, oversized records, backlog, discovery
limits, file-open errors, and evictions. File-open failures are cumulative attempts;
read failures are attached to an open source. Source IDs and execution labels are
local to one run and are not durable identifiers for exporting evidence.

## Supported sources

| Client | Automatic macOS discovery | Recognized evidence |
| --- | --- | --- |
| Codex Desktop | `~/Library/Logs/com.openai.codex/YYYY/MM/DD/*.log` (also root `.log` files) | Structured MCP starting/ready/failed/cancelled statuses; app-tool discovery failures; explicit auth, timeout, missing executable, and transport reasons |
| Claude Desktop | `~/Library/Logs/Claude/mcp.log`, numbered `mcpN.log` rotations, and `mcp-server-*.log` | Startup/connection lifecycle; intentional shutdown, renderer release, unexpected disconnect; dedicated-server errors; allowlisted protocol methods |

Both Claude prefix orders, full JSON and metadata-only protocol messages are
recognized. A 50 ms cross-source mirror window accounts for observed timestamp
skew; each occurrence pairs once. Deduplication is limited to retained events.
Claude execution inference requires a retained startup in the same file generation
or its proven mirror. Missing scope stays uncertain. Codex thread/host identifiers
are hashed into process-local labels and never serialized directly.

The [local audit](MCP-LOG-AUDIT-2026-11-12.md) documents the verified log shapes.
New client versions may change them; recognition counters expose that limitation.
Claude application `main*.log`, Codex CLI, Claude Code, OpenCode, remote-server
logs, automatic Linux discovery, and Windows are not supported adapters yet.
There is no complete request/result trace, inferred timeout, tool error-rate
calculation, or request-ID correlation. Available protocol methods are metadata;
payloads and tool outcomes remain unavailable.

## CLI options

```sh
tell-tail mcp                          # Text events and periodic status
tell-tail mcp --output json            # MCP schema 1 NDJSON
tell-tail mcp --output web             # Print URL without opening a browser
tell-tail mcp --open --web-port 8787
tell-tail mcp --source claude-desktop --open
tell-tail mcp --source codex-desktop --log-dir /path/to/logs --open
tell-tail mcp --once --output json     # Inspect bounded history, then exit
tell-tail mcp --demo --once --output json
```

`--log-dir` requires `--source`. Explicit roots use the same selectors and are
usable for controlled captures on Linux. `--demo` conflicts with source options.
`--once` is headless only. Builds without the `web` feature support text and JSON
and reject `--open` with an actionable error. Missing directories are waiting
states; discovery continues. Paths named literally `mcp` remain readable with
`tell-tail ./mcp` or `tell-tail -- mcp`. General log-mode options do not apply to
the MCP command; `tell-tail mcp --help` lists its options.

## Local data contract and limits

The loopback server serves only embedded assets and read-only endpoints, with
Host/Origin checks, CSP, no-store responses, 16 connections and four SSE streams.
Slow writes have a two-second no-progress deadline. It does not call MCP servers
or send logs to an external service. Default data includes bounded server labels,
fixed diagnostic descriptions, method names, opaque execution labels and offsets.
Raw message bodies, URL queries, raw execution identifiers, tool arguments and
results are discarded. Labels are conservative ASCII allowlists; known token
prefixes are rejected. Do not put secrets in server names.

| Endpoint | Contract |
| --- | --- |
| `GET /api/mcp/snapshot` | MCP `schema_version: 1`, sequence, connection/source state, counts, at most 100 recent arrival-order events, 40 recent issues and 40 states; at most 64 KiB |
| `GET /api/mcp/events` | SSE `snapshot` events, latest-state coalescing at most 5 Hz; 15-second idle comments; reconnect gets current state, not durable replay |
| `GET /api/mcp/history` | Source-time descending event pages (unknown times use receipt time); 100 rows and at most 64 KiB |
| `GET /api/mcp/event/{id}` | Safe retained event, at most 16 KiB; HTTP 410 after eviction |

History accepts `q`, `host`, `server`, `category`, `since` (Unix milliseconds),
`before` (previous page's `next_before` ID), and `until` (inclusive receipt ID,
used to freeze a view). `category=failure` selects all explicit failures; otherwise
it selects a serialized reason. Duplicate/malformed query keys return HTTP 400;
an expired cursor returns 410. Payload availability is always false. Source times
are parsed only as verified UTC forms; unrecognized timezone forms remain undated.
NDJSON emits `event`, periodic/final `sources`, and explicit retention `gap`
envelopes. Existing general-log CLI schema 2 and web schema 1 are unchanged.

Discovery visits at most 4,096 directory entries and retains 512 candidates,
reserving capacity across clients, with 64 active readers. Each reader gets a
64 KiB turn, a 256 KiB framer (16 MiB aggregate ceiling), and an independent
file-generation boundary. Bootstrap shares 16 MiB across selected files, capped
at 2 MiB each; omitted bytes are visible. The collector follows appends, rescans
once a second, drains renamed descriptors, and resets on observed truncation.
A truncate-and-regrow between polls can hide lost bytes; it is not recoverable
from a passive watcher. File identity checks and no-follow opens reject source
replacement and final-component symlink races.

Memory retention is capped at 2,000 events, 1,024 issue groups and 1,024 server
states. Retained strings are bounded labels or fixed enums/descriptions, not raw
256 KiB records; together they remain below the plan's 8 MiB evidence allowance.
The browser explains evictions and overview truncation. Full disks or unavailable
source directories do not trigger a persistent-log fallback.

## Validation

See [MCP implementation validation](VALIDATION-MCP.md) for measured results,
remaining external validation, and reproducible checks. The two existing 24-hour
soak scenarios remain optional, non-blocking background checks.

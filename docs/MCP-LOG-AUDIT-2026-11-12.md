# Local MCP log feasibility audit

Inspected November 12, 2026, on one macOS workstation. Existing local logs are
sufficient for a useful passive MCP diagnostics watcher, especially for Codex
Desktop startup and authentication failures. They do not establish complete
tool-call visibility across clients. No server configurations, credentials,
logging settings, or application processes were changed.

The evidence supports building `tell-tail mcp --open` around automatic discovery,
source freshness, server attribution, and grouped failures. A packet capture
proxy, agent attention dashboard, and application-change correlation are outside
this investigation.

## Sources inspected

Paths below use `~` instead of the workstation account name. Counts describe
separate read-only scans of changing files, not a simultaneous system snapshot.
The [aggregate evidence](validation/mcp-log-audit-2026-11-12.json) contains no raw
payloads, credentials, conversation IDs, or actual server identifiers.

| Source | Observed files and scope | Result |
| --- | --- | --- |
| Codex Desktop | 64 `.log` files below `~/Library/Logs/com.openai.codex/`, organized by date; approximately 75 MB at inventory | Strong structured MCP startup, authentication, discovery, and catalog signals; current logs append |
| Claude Desktop dedicated MCP logs | `~/Library/Logs/Claude/mcp.log` plus three `mcp-server-*.log` files | Useful connection lifecycle and one unexpected disconnect; aggregate and server logs mirror each other |
| Claude Desktop application logs | `main.log`, `main1.log`, `main2.log` in the same directory | Historical remote tool-call starts and extension cache errors; mixed with large amounts of unrelated and multiline data |
| OpenCode | `~/.local/share/opencode/log/opencode.log`, 119 lines | No MCP mentions or MCP failure evidence in this sample |
| Conventional CLI log locations | `~/.codex/log` and `~/.claude/debug` | Absent on this workstation; CLI adapters were not verified |

Claude's installed application reports version `1.24012.9`. That does not identify
the versions that wrote every historical record. No Codex binary version was
established; adapters must therefore identify the observed log shapes rather than
claim compatibility with an unverified version range.

## Codex has actionable failures already

The complete Codex scan contained 162,875 physical lines. Of these, 15,703
mentioned MCP, but that substring count includes unrelated context and is not
an event count. Explicit `mcp_server_startup_status_updated` records contained:

| Status | Recorded occurrences |
| --- | ---: |
| `starting` | 1,944 |
| `ready` | 2,927 |
| `failed` | 666 |
| `cancelled` | 3 |

There were also 1,339 explicit `Failed to discover app tools` records. These
counts include repeat reporting across sessions and discovery attempts; they
must not be presented as independent incidents or used to calculate a tool-call
failure rate.

The files in the `2026/11/12` directory contained 28 failed startup records and
90 discovery-failure records. This directory is a UTC date partition, not a
Pacific-time calendar day. All 28 startup failures identified two servers with
`failureReason=reauthenticationRequired`; the latest sampled instances occurred
around 11:02 AM Pacific on November 12. The messages explicitly recommend logging
in again. Other historical errors included authentication-required responses,
rejected OAuth refreshes, and OAuth metadata discovery/network failures.

A watcher can turn these records into an attributable issue with a reason,
first/last occurrence, affected execution scopes, and evidence. It should retain
the distinction between a failed startup and a failed catalog lookup. A generic
`Loaded MCP server catalog` event does not demonstrate that every server recovered.

The format is timestamp, level, component, event name, then key/value fields.
Quoted values contain escaped newlines and nested error text. Successful records
can include `error=null`; keyword matching would misclassify them. One physical
line in the full application-log sample was 100,296 bytes, so bounded parsing and
explicit truncation handling are necessary.

## Claude provides lifecycle evidence with duplication

The aggregate MCP log contained 675 lines, and the three server logs contained
675 lines in total. After removing the per-server metadata suffix and normalizing
prefix order, their message multisets matched completely. Pairing occurrences by
server, severity, message, and order found corresponding timestamps within 31 ms;
674 of 675 were within 10 ms. Exact byte or timestamp equality is insufficient.

The per-server records contained 43 initialization messages, 43 connected
messages, 34 explicitly intentional closes, eight renderer-release closes, and
one explicitly unexpected server-transport close. The unexpected close was
followed by an error-level disconnect. These describe one failure sequence,
not two separate incidents. The unexpected close itself was logged at info level.

The dedicated logs ended on July 24, 2026. They are historical evidence, not proof
of current server state. A UI that opens them and shows an unqualified green
connected badge would be misleading.

Older records include JSON-RPC payloads. Later records in the PDF viewer log,
first observed on June 29, contain metadata summaries such as method and request
ID with omitted payload details. Both shapes need fixtures. The dedicated logs
showed initialization, tool listing, and resource listing, but no `tools/call`
records or `isError: true` results in this sample.

The older application logs contained 457 `Making remote MCP tool call` starts
and 130 PDF-extension cache-update failures with missing-file errors. This adds
useful evidence, but it does not supply a complete matched tool-call/result stream.
An extension cache failure must be labeled as a client/extension issue rather
than a failed server request. Supplemental application-log support should follow
the dedicated adapters and include multiline boundaries and strict event matching.

## Passive observation and existing binary checks

A temporary read-only observer watched known log files and rediscovered matching
files for 60 seconds, polling once per second. Codex appended 1,441 bytes in five
lines. None of those five lines was an MCP event. Claude and OpenCode did not
append, and no rotation or new file was observed during that interval.

This verifies live file access and appends on this machine. It does not measure
MCP-event latency, establish rotation correctness, or prove end-to-end live
failure detection. Those need controlled replay and subsequent live acceptance.

The pre-existing `target/debug/tell-tail` binary scanned each of two files with
`--once --output json`, returning exit 0, no parse failures, and no oversized
records. Claude's aggregate file produced 674 info and one error record. The
then-current Codex file produced 4,387 records: 2,993 info, 650 warning, and 744
error. Those Codex totals include non-MCP application events. This was a smoke
check of the existing binary, not a clean-build validation of the current source
or an implemented MCP adapter.

## Product decision

Proceed with a macOS-first passive watcher for the verified Codex Desktop and
Claude Desktop formats. Lead with a live, filterable evidence timeline and
grouped MCP issues. Show the last observed server state separately from source
freshness and watcher status. Use historical context on startup so a quiet
application does not produce an unexplained blank screen.

Do not promise comprehensive requests, latency, current health, or a failure
percentage from these logs. OpenCode and terminal clients need their own observed
fixtures before receiving a supported-adapter label. Recreate sanitized examples
for tests; do not commit the original logs or copied tool payloads.

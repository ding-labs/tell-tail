# Passive MCP log watcher plan

Status: proposed November 12, 2026. Local feasibility has been inspected; the MCP
command and adapters are not implemented. The [local audit](MCP-LOG-AUDIT-2026-11-12.md)
establishes useful existing log signals and their limits.

## Outcome

```sh
tell-tail mcp --open
```

One command opens an embedded local dashboard showing MCP activity and failures
already recorded by supported applications. Developers can identify the affected
server, understand the reported failure, inspect nearby evidence, and see whether
a later observation indicates recovery. No account, model key, server wrapper,
configuration edit, debug-mode change, or application restart is required.

The first supported discovery targets are Codex Desktop and Claude Desktop on
macOS. Preserve existing Tell-Tail behavior and distribution targets. Explicit
log-directory inputs and synthetic fixtures enable testing on supported Linux
builds; that does not establish automatic Linux client discovery. Windows,
OpenCode, and terminal-client discovery require verified fixtures and platform
testing before being advertised. The inspected OpenCode log contained no MCP
events, and conventional Codex/Claude CLI log directories were absent.

## Product experience

Start watching immediately, load a bounded recent tail, and open the browser once
the local server is listening. Distinguish imported history from events received
after startup. The initial page shows discovered sources and their last write
times even before any MCP event has been parsed. If no supported logs exist,
keep discovery running and explain exactly which locations were checked.

The default view contains a live timeline and grouped issues, with filters for
client, server, failure category, and time. Each issue shows a concise reason,
first and last observation, recurrence count, affected execution scopes, and a
link to supporting events. Repeated messages update an issue instead of filling
the entire screen. A developer can pause display updates, search retained events,
inspect details, and copy a redacted diagnostic summary without stopping capture.

Illustrative issue, using a synthetic server name:

```text
Codex Desktop / example-api
Last reported failure: OAuth reauthentication required
Seen in 3 executions · 12 related log observations
Last observed 18 seconds ago · watcher connected

Evidence: startup status failed; failureReason=reauthenticationRequired
Next step: sign in to this server again in the host application
```

Remediation is deterministic and based on recognized failure categories. Show
source evidence separately from inferred advice. Do not execute repair commands
or treat commands embedded in logs as instructions. LLM explanations, notifications,
cost tracking, proxies, and correlation with application edits are later ideas,
not dependencies of this command.

## Evidence and interpretation

| Verified signal | First implementation behavior | Boundary |
| --- | --- | --- |
| Codex structured startup statuses and failure reasons | Track starting, last reported ready, failed, and cancelled by server and execution scope | A ready event is an observation, not a continuous health check |
| Codex app-tool discovery failures | Group authentication, transport, and discovery failures with their original phase | Catalog loading does not prove a particular server recovered |
| Claude startup, connected, and transport-close records | Show a lifecycle timeline; distinguish intentional shutdown, renderer release, and unexpected close | Generic closure alone is not a crash |
| Claude aggregate and server-log mirrors | Display one event with multiple evidence references when matched | Preserve repetitions within one source and events in separate sessions |
| Claude payload and metadata-only record variants | Extract available method/ID metadata; explicitly mark unavailable payload/outcome | No complete tool-call log or error-rate denominator was demonstrated |
| Old or silent source | Display last write time and last observation time | Silence is neither healthy nor failed |
| Unrecognized MCP record | Retain bounded, sanitized evidence and show recognition coverage | Do not invent a semantic failure from an arbitrary error word |

Supplemental Claude `main*.log` parsing is a subsequent adapter increment. Limit
it to recognized remote MCP call and extension-diagnostic records with bounded
multiline context. The audit found historical call starts and extension cache
failures there, but not a verified complete request/result stream. Label extension
installation/cache failures separately from server startup and tool failures.

Separate three concepts throughout the UI and data model: watcher connection,
source freshness, and last observed server state. A successful observation in one
execution must not clear a failure in another. Where scope cannot be determined,
retain uncertainty rather than merging records into a fictitious global session.

Use source timestamps for historical ordering and elapsed event durations, while
keeping ingestion timestamps and a stable sequence for replay and pagination.
Do not calculate live rates from imported history. Timestamps without a timezone
carry an explicit timezone assumption. Clock reversal, log gaps, process restarts,
and reused request IDs must not produce negative durations or cross-session pairs.
Absence of a response is an unknown outcome unless the host explicitly logs a timeout.

## Discovery and collection

| Adapter | Initial source roots and selectors |
| --- | --- |
| Codex Desktop | `~/Library/Logs/com.openai.codex/YYYY/MM/DD/*.log`; multiple process files can coexist |
| Claude Desktop | `~/Library/Logs/Claude/mcp.log` and `mcp-server-*.log`, with explicitly recognized rotation names |
| Explicit source | A user-supplied log directory with a selected adapter; use the same file checks and limits |

Use shallow, adapter-specific discovery, not a recursive scan of the home folder.
Read regular log files only. Do not read credentials, conversation stores, or
arbitrary configuration files to build the basic view. Canonicalize discovered
paths and prevent symlinks from expanding an automatic root into unrelated data.
Source access errors remain visible per source while other sources continue.

Maintain a fair scheduler over open files and an independent framer for each
file generation. Read appended bytes with partial-record handling, rediscover
new files/date directories, and identify files by filesystem identity rather
than path alone. Renamed files drain before replacements take over. Truncation,
deletion/reappearance, and generation changes reset framing and session evidence
without joining unrelated bytes. Report any gap a copy-truncate race makes
observable; do not claim recovery of bytes never read.

Bootstrap from the end of the most recent files under a global byte budget,
skipping an incomplete first record. Take a per-file end offset, read history up
to it, then follow from that offset so appends during bootstrap are neither
omitted nor replayed. Historical rotated files can supply bounded recent context;
do not attach unlimited followers to every archive. Importing an old Claude log
must visibly say historical rather than presenting its last state as current.

Prefer Claude's per-server record as the canonical display when its aggregate
mirror is also present. Match only across known mirror sources using normalized
server, level, message, and a small timestamp tolerance validated by fixtures.
The local sample needs at least 31 ms; begin with a 50 ms tolerance and test
nearby legitimate repetitions. Pair each occurrence at most once, retain both
provenance references, and preserve aggregate-only events. Failure grouping is a
separate operation and must not alter the count of distinct observed records.

## Data and architecture

```text
adapter-specific discovery
  -> bounded multi-file readers and per-generation framing
  -> host-specific parsers
  -> sanitization, provenance, mirror deduplication
  -> bounded events, execution state, and grouped issues
  -> read-only snapshots and bounded detail lookup
  -> local browser or headless output
```

Reuse the existing framing, file-identity checks, rotation behavior, cancellation,
browser opener, loopback access restrictions, and coalescing publication patterns.
Keep a separate `src/mcp/` model and runtime rather than forcing server/session
semantics into severity `Stats`. Extract shared follower primitives from
`src/input.rs` surgically, with existing behavior covered by regression tests.
Avoid one unbounded thread or queue per discovered file.

An MCP event needs schema version, source and generation IDs, host adapter,
optional server and execution scope, source/ingestion times, semantic kind,
original severity, structured reason, a bounded evidence preview, provenance,
and explicit coverage/truncation flags. Preserve file offsets as evidence locators;
never pretend a tail-relative line number is an absolute line number.

Group issues by host, server, execution scope, phase, and normalized failure
reason. A presentation rollup can combine scopes while retaining their individual
states. Report observation counts and affected scopes, not an inferred number of
unique outages. Model recovery only from affirmative, matching evidence. Keep
historical and unconfirmed issues accessible without asserting they remain active.

Known secret-bearing fields, authorization material, URL credentials/query data,
and tool arguments/results must not enter the default public snapshot. Default
evidence previews use an allowlist of recognized diagnostic fields; sanitize
free-form excerpts before retention. Preserve bounded opaque metadata for unknown
records without forwarding their full payload. Payload bodies stay hidden in the
initial release. Log strings render as text, never HTML or executable commands.
Do not add automatic uploads, persistent raw-log copies, or an inference service.

Introduce a versioned MCP-specific web contract and NDJSON event/source envelopes.
Existing CLI JSON schema 2 and web schema 1 retain their meanings. SSE publishes
a bounded overview and recent event page; a bounded detail endpoint returns only
sanitized retained evidence by opaque event ID. Do not serialize the whole event
ring for every update. Reconnection receives current state and exposes sequence
gaps or eviction rather than implying a durable complete history. Retain current
loopback binding, Host/Origin validation, CSP, and slow-client isolation.

## Resource budgets and performance targets

These are initial design budgets and acceptance targets, not measurements of an
implemented watcher. Tune them only with recorded evidence and visible overflow
behavior.

| Resource or behavior | Initial target |
| --- | --- |
| Startup history | Up to 2 MiB per source and 16 MiB total; mark omitted history |
| Discovery | At most 512 candidate files per scan and 64 active sources; disclose skipped candidates/sources |
| Physical record | At most 256 KiB per source framer; count oversize records and recover at the next boundary |
| Retained events | At most 2,000 events and 8 MiB of evidence; evict oldest with visible counts |
| State/group cardinality | At most 1,024 execution/issue entries under a separate byte budget; explicit eviction/overflow |
| Browser data | At most 100 compact rows per view, 64 KiB overview frame, and 16 KiB sanitized detail response |
| Publication | Coalesce on changes, at most 5 updates/second; collector never waits on a browser |
| Discovery latency | Recognize a new supported file within 2 seconds under the benchmark workload |
| Appended event latency | p95 under 500 ms from completed line write to browser visibility, excluding host buffering |
| First usable view | Under 2 seconds for 32 source files and a 16 MiB bootstrap workload |
| Idle cost | Below 1% of one CPU core and 128 MiB RSS on the reference Apple Silicon machine |
| Shutdown | Preserve the existing one-second cancellation target |

Use a global framer budget as well as per-source limits. Budget state strings,
deduplication caches, parser nesting depth, directory traversal, and temporary
encoding buffers. On saturation, report dropped/evicted evidence, incomplete
coverage, and backlog. Neither a noisy file nor slow rendering may starve quieter
sources or grow memory without bound. Avoid rereading entire logs on each tick.

## Command behavior and compatibility

`mcp --open` selects web output and opens the loopback page. Without `--open`,
`mcp` emits a concise text stream/status view; `--output json` supplies normalized
NDJSON for automation, and `--output web` prints a URL without opening it.
Support `--web-port`, a host/source filter, and an explicit adapter log directory
for controlled captures. Keep the basic command free of mandatory options.

Adding a subcommand reserves the positional name `mcp`. Preserve access to a file
literally named `mcp` through `tell-tail ./mcp` and `tell-tail -- mcp`, and test
both. Existing file/stdin, demo, Apple unified log, output, and webhook invocations
must keep working. Validate incompatible flags rather than silently ignoring them.
Without the `web` feature, reject `--open` with an actionable message while keeping
the headless MCP modes available. An opener failure leaves a usable printed URL.

No logs is a waiting state, not a successful-health claim or a fatal error. A
single unreadable source does not kill the watcher. Binding errors and invalid
arguments do fail clearly. Ctrl+C stops collection and serving; closing the browser
does not secretly stop the terminal-owned process. No background service install
or server configuration write is part of this command.

## Implementation increments

Keep each increment reviewable and verified, and commit it before starting the
next independent change. Split rows further when needed; do not accumulate the
whole feature into one commit.

| Order | Change | Required verification |
| --- | --- | --- |
| 1 | Define MCP event/source contracts and hand-authored, sanitized format fixtures | Assert exact expected semantics, missing fields, malformed inputs, and no private fixture data |
| 2 | Implement bounded discovery and source identity | Temporary directory trees, absent roots, permissions, symlinks, dated directories, candidate limits |
| 3 | Extract shared follower primitives and implement fair multi-file collection | Existing follower regressions plus interleaved chunks, new files, rotation, truncate, and bootstrap/append races |
| 4 | Implement Codex Desktop adapter | Starting/ready/failed/cancelled, reauthentication, discovery failures, quoted values, `error=null`, unrelated app events |
| 5 | Implement Claude dedicated-log adapter and mirror matching | Both prefix orders, payload/metadata variants, 31 ms skew, repeated legitimate messages, intentional/renderer/unexpected closes |
| 6 | Add grouping, execution state, freshness, redaction, and bounded retention | Same server across executions, explicit recovery, stale history, clock changes, eviction, sensitive and adversarial text |
| 7 | Wire CLI, lifecycle, text and NDJSON modes | Exact command dispatch, literal `mcp` filenames, existing flag compatibility, empty discovery, partial failures, feature builds |
| 8 | Add MCP web snapshots, detail lookup, and SSE | Schema limits, reconnect/gaps, access controls, slow clients, publisher failure, no raw payload leakage |
| 9 | Build the timeline, issue view, source coverage, and diagnostic copy flow | Browser tests, keyboard access, pause/search/filter behavior, historical labeling, misleading-health regressions |
| 10 | Add a synthetic MCP demo and perform installation/performance acceptance | Packaged binary offline, opener failures, end-to-end replay, resource budgets, manual live checks |

The optional Claude application-log adapter and additional client/platform adapters
follow this vertical slice. A plugin can later offer discovery and evidence tools
around the same read-only engine; plugin installation is not required for launch.

## Acceptance and release evidence

Use synthetic logs recreating observed formats, with a replay writer in temporary
directories. Inject missing executables, auth failures, explicit timeouts, malformed
records, normal shutdowns, unexpected closes, and successful recovery. Replay partial
writes, bursts, duplicate mirrors, cross-session concurrency, rotation, and late
events through the actual CLI and browser. Unknown raw outcomes must remain unknown.
Do not induce faults in the user's real MCP servers or alter their configuration.

Required functional acceptance:

1. A fresh supported installation runs `tell-tail mcp --open` with no setup and
   discovers the verified desktop sources, or presents an accurate waiting screen.
2. A seeded reauthentication failure yields an attributable issue, its original
   structured reason, affected scope, last observation, and safe next step.
3. The 675 paired synthetic Claude records yield 675 displayed observations, not
   1,350, while genuine repeated events survive. The unexpected-close sequence
   groups into one issue; the 42 normal close sequences create no failure issue.
4. Old Claude records are historical, and source freshness never masquerades as
   MCP health. A currently readable Codex log can still contain an old failure.
5. Normal app noise, `error=null`, and unrelated extension/UI problems do not
   become server failures. Unknown formats remain visible as coverage limitations.
6. Tool arguments, results, credentials, and session identifiers do not leak into
   default browser snapshots, copied summaries, or test artifacts.
7. The installed binary meets measured latency/resource targets and stays usable
   under overload, partial source failures, rotation, and slow/disconnected tabs.

Run the appropriate Rust feature-matrix, CLI compatibility, browser, packaging,
and performance checks described in [dashboard validation](TESTING-DASHBOARDS.md).
Verify the actual built binary, not a stale target. Preserve results and the tested
client/log-shape compatibility matrix. A subsequent passive live run must observe
a real MCP event before claiming live MCP failure detection verified; the audit's
60-second append check did not do that. Controlled replay provides repeatable
failure coverage without requiring a real service to fail before release.

Pilot with five developers who use supported clients. Record install-to-first-view
time, whether they can identify a seeded failure within one minute, false issue
reports, and missing evidence. Fix correctness or onboarding failures before
expanding adapters. This checks usefulness, not just parser throughput.

The two 24-hour stdin/NDJSON and followed-file/metrics soaks remain optional,
non-blocking background validation. They never gate CI, publishing, deployment,
or release and require no per-release waiver. Retain their results and investigate
actionable failures separately, as specified in the repository instructions.

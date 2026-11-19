# Per-plugin tool-call tracing implementation plan

Status: result-only desktop delivery implemented November 19, 2026. The live
capture checkpoint verified attributable Railway results for sequential and
parallel calls inside `functions.exec`. The OTLP adapter, authenticated receiver,
headless output, read-only API/SSE, bounded Tool calls workspace, frozen inspection,
and offline browser replay are implemented. See the [capture audit](PLUGIN-TRACE-CAPTURE-AUDIT.md)
and [setup guide](PLUGIN-TRACING.md).

Coverage remains deliberately scoped: start/cancellation/subagent/provider-error
semantics and direct model calls are unverified; plugin mappings beyond the exact
observed Railway namespace are not inferred. Render-latency benchmarking remains
separate from receipt-to-query measurements. These are limitations of this delivery,
not claims that every installed plugin and execution path has been verified.

## Outcome and scope

When a supported client invokes a plugin tool, Tell-Tail shows one attributable
call with its tool name, observed lifecycle, duration when available, and outcome.
The row updates as evidence arrives. The developer can filter by plugin, inspect
the call, and copy diagnostic metadata without copying the tool's payload.

The first real acceptance case is a read-only Railway tool invocation from the
user's desktop chat. Use an available metadata or documentation lookup first;
account and balance access is not necessary to establish tracing. Record the
actual tool identifier exposed by the client rather than inventing a balance API.

Initial scope:

- Codex Desktop on the verified local execution path, including hosted plugins.
- Individual calls made directly and through `functions.exec`, when exposed by
  the selected source. A wrapper event alone does not meet the nested-call goal.
- Existing passive MCP diagnostics alongside a separate tool-call view.
- Bounded local collection, browser inspection, and normalized NDJSON output.
- Explicit partial coverage when a client supplies only results or omits identity.

Not in the first release: a universal MCP proxy, modifying third-party plugins,
full payload retention, distributed tracing inside Railway or other providers,
remote collectors, automatic billing/cost attribution, long-term trace storage,
or support claims for untested clients. One tool call may make several internal
service requests; Tell-Tail will not infer those requests from the tool result.

"Every call" is an acceptance target for a named, tested execution path. It is
not a promise about all installed plugins, all chats, or all client versions.

## Existing code and verified upstream options

The current watcher reads supported desktop log files through `src/mcp/collection.rs`
and `src/mcp/parse.rs`. Its store groups lifecycle failures and retains bounded
evidence. It does not retain request IDs or correlate individual tool calls.
See [current coverage](MCP.md#supported-sources) and [validation](VALIDATION-MCP.md).

The dashboard now has a timeline-first workspace, a separate failure table,
bounded scrolling, and an evidence inspector. Extend that structure rather than
adding a growing list of plugin cards above the timeline.

Upstream documentation establishes candidate mechanisms, not compatibility with
the user's installed app:

| Candidate | Documented capability | What still needs verification |
| --- | --- | --- |
| Codex OpenTelemetry export | Structured tool-result events include duration and success; export is configurable | Desktop configuration scope, individual nested calls, identity, call IDs, delays, and outcome semantics |
| Codex app-server events | MCP call items can carry plugin/app identity, tool, status, duration, and started/completed lifecycle | A supported way to observe this running desktop app without owning or resuming its chats |
| Codex lifecycle hooks | Pre/post tool hooks are documented, with different support for local and cloud orchestration | Availability, nested-call coverage, failure/cancellation events, IDs, overhead, and configuration scope |
| Existing passive desktop logs | Useful connection, authentication, and discovery evidence | They do not establish complete per-call coverage |

Sources checked November 18, 2026:
[telemetry](https://learn.chatgpt.com/docs/config-file/config-advanced#observability-and-telemetry),
[app-server](https://learn.chatgpt.com/docs/app-server), and
[hooks](https://learn.chatgpt.com/docs/hooks).
Recheck the relevant source and installed schema during implementation. Do not
assume a separately launched app-server sees the desktop app's existing activity.
Local execution also does not establish local orchestration or local hook support.

## Phase 0: prove the capture path

This is the first implementation milestone and a decision checkpoint, not an
excuse to implement three production adapters at once.

1. Record desktop version, bundled execution engine version where available,
   OS, local/cloud orchestration, relevant policy restrictions, and the exact
   scope the proposed collector can see. Avoid reading credentials or unrelated
   chat history to discover this information.
2. Test OTel as the first candidate using a bounded, loopback-only probe. Inspect
   only allowlisted fields and field presence; never write raw batches to disk.
   Document the configuration diff, whether a restart is required, and rollback.
   This planning task does not change the user's configuration.
3. Make a known sequence of harmless plugin calls. Compare the emitted records
   with the known invocation count and client-visible results. Count tool discovery
   separately from tool execution.
4. Test direct calls, two sequential calls in one `functions.exec`, parallel calls,
   a failing call, cancellation, and a subagent call where that source supports it.
   Use synthetic/local tools for intentional failures and cancellation. Do not
   induce failures in the user's real services.
5. Check whether the records include a stable call ID, execution/session scope,
   plugin/connector identity or resolvable tool name, timestamps, duration,
   structured outcome, and parent call ID. Missing fields remain missing.
6. If OTel misses nested calls or attribution, evaluate a documented app-server
   observer connection. Reject approaches that take over chats, handle approvals,
   mutate/resume threads, or depend on a private IPC interception mechanism.
7. Evaluate supported hooks only if needed. A diagnostic handler must not emit
   blocking decisions, alter tool inputs, or inject model context. Verify failure
   behavior and bounded overhead before considering it a viable capture source.
8. Select one adapter and record which capabilities are proven. If all candidates
   expose only the wrapper, report the missing upstream instrumentation; do not
   parse executable code or ask the model to fabricate child traces.

Deliverables: `docs/PLUGIN-TRACE-CAPTURE-AUDIT.md`, a metadata-only compatibility
record under `docs/validation/`, and synthetic fixtures recreating the verified
event shapes. No user payloads, raw IDs, balances, or tokens enter those artifacts.

The decision must be one of:

- **Lifecycle capture:** individual plugin calls, attributable identity, and
  reliable start/completion correlation are verified. Proceed with live states.
- **Result-only capture:** individual results and attribution are verified, but
  starts are absent. Ship completed-call tracing with an explicit capability label;
  do not show invented running states or reconstruct starts from arrival times.
- **Partial capture:** some execution paths or plugins are visible. Scope the
  adapter and product claims to those paths and name the omissions.
- **No viable desktop feed:** retain the passive watcher and document the required
  client support. A separate CLI integration is a separate scope, not completion
  of the desktop feature.

## Capture architecture

```text
selected client event source
  -> bounded decoder and adapter
  -> field allowlist + local identity mapping
  -> bounded normalized event queue
  -> call correlator + revision history + coverage state
  -> paginated query / bounded SSE / text and NDJSON
  -> Tool calls view and diagnostic inspector

existing desktop log readers -> existing MCP lifecycle store -> same dashboard
```

Add a source-independent `src/tool_trace/` module. Keep transport decoding out of
the existing line parsers and do not force call lifecycles into severity statistics
or MCP failure groups. Reuse small, proven transport/publication helpers where
appropriate; avoid a broad networking refactor as a prerequisite.

Proposed module ownership:

| Location | Responsibility |
| --- | --- |
| `src/tool_trace/model.rs` | Normalized input events, public call records, capability and coverage types |
| `src/tool_trace/adapters/` | Only verified source/version decoding; payload removal before retention |
| `src/tool_trace/identity.rs` | Plugin resolution and process-local opaque execution identifiers |
| `src/tool_trace/store.rs` | Correlation, bounded revisions, deduplication, retention, queries |
| `src/tool_trace/runtime.rs` | Queue processing, source status, cancellation, publication |
| `src/tool_trace/receiver.rs` | Selected network transport, if the audit chooses export-based capture |
| `src/mcp/cli.rs`, `src/mcp/runtime.rs` | Explicit opt-in and coordination with passive collection |
| `src/web/mcp.rs` plus a small trace route module | Read-only browser API and SSE integration |
| `web/mcp.html`, `web/mcp.css`, a trace-specific JS module | Tool-call table, filters, inspector, coverage |
| `tests/tool_trace_*.rs`, `browser-tests/tool-trace.test.mjs` | State, transport, CLI, and real-browser acceptance |

The adapter is selected after Phase 0. If OTel wins, implement the exact OTLP
encoding emitted by the tested client first, using pinned protocol definitions
compatible with Rust 1.88.0. Do not describe a custom JSON endpoint as OTLP.
Defer additional encodings and gRPC until a verified client requires them.

## Normalized contract and identity

Use an independent trace schema version, initially `1`. Keep existing MCP schema
1 and general log schemas unchanged. All network/file inputs are untrusted data.

| Field group | Required interpretation |
| --- | --- |
| Envelope | Trace schema version, source ID, source epoch, ingestion sequence, event kind |
| Execution | Opaque local execution label; optional opaque turn and parent-call labels |
| Call identity | Source call ID transformed to a local label; explicit missing-ID state |
| Attribution | Client, server, tool, plugin/connector IDs when present, display name, resolution basis |
| Timing | Source timestamp, receipt timestamp, optional duration, and duration basis |
| Lifecycle | Started, completed, failed, cancelled, denied, or source-supported result event |
| Outcome evidence | Source execution status, structured tool-error flag when available, sanitized error category |
| Provenance | Adapter/version, source event identity where available, live/imported origin, gap/conflict flags |

Internally correlate by `(source identity, source epoch, execution scope, call ID)`.
Validate that source reconnects and process restarts produce the appropriate epoch;
do not use a new TCP connection alone as proof of a new execution. Keep identifiers
opaque in public output, using per-run keyed mapping. Do not hash raw payloads to
produce identity. Labels are not stable across Tell-Tail restarts in v1.

Resolve plugin identity in this order:

1. Explicit plugin or connector metadata from the event.
2. A verified catalog mapping for the same client/source generation.
3. A documented, fixture-tested tool namespace mapping, labeled as derived.
4. Unidentified plugin, retaining the safe tool/server label if available.

Do not assume `codex_apps` is a plugin or split arbitrary underscore-delimited
names to guess a provider. A plugin can expose multiple servers and tools. Keep
plugin identity separate from the transport server. Apply length and cardinality
limits to mappings, and preserve the mapping basis used by older records.

If a source lacks stable call IDs, retain uncorrelated observations explicitly.
Do not pair identical tool names by proximity. Cross-source deduplication requires
a proven common identifier; otherwise use one authoritative call source and show
the other source as separate evidence. A real retry gets its own attempt record.

## Lifecycle and outcome rules

| Input sequence | Display and accounting |
| --- | --- |
| Start then completion | One call, two revisions, final duration and outcome |
| Completion without start | One result-only call; source duration may still be shown |
| Late start after completion | Enrich the same call without reverting its terminal state |
| Duplicate delivery | No additional invocation; count duplicate transport records separately |
| Explicit cancellation/denial | Cancelled/denied; do not count denial as executed success or failure |
| Source disconnect or shutdown with open calls | Outcome unknown/incomplete; never manufacture timeout failures |
| Terminal event after a known gap | Apply authoritative evidence but retain the coverage gap |
| Conflicting terminal events | Preserve conflict evidence; do not silently choose a convenient outcome |
| Same tool invoked again with a new call ID | Separate call, even if inputs and timing appear identical |

Keep execution status separate from observation completeness. A disconnected
collector cannot assert that the remote tool stopped running. An aging unmatched
start becomes incomplete/unknown after the documented retention policy, not failed.

Prefer source-reported duration. Otherwise compute it only from compatible start
and completion timestamps in the same execution clock domain. Reject negative or
overflowed values and label unavailable duration. Receipt-time differences are not
tool latency; export batching can delay delivery. Do not derive parent/child links
from timing alone.

Verify whether source duration includes approval waits, queueing, or retries.
Label that scope in the inspector; do not call an end-to-end duration server
execution time. Show waiting-for-approval only when the source reports it.

"Completed" is not automatically "successful." Establish how the selected source
represents protocol errors, MCP `isError`, and tool/application failure. Store
execution completion and reported tool outcome separately. If the source exposes
only execution success, label it accordingly. Do not classify arbitrary text such
as an account status or a balance of zero as a tool error.

Future rates must use only observed calls with known outcomes and show the
denominator. Unknown and denied calls stay separate. Do not calculate a plugin
health score, failure rate, or capture percentage from incomplete source evidence.

## Local transport, setup, and privacy

Passive `tell-tail mcp --open` retains its no-setup behavior. Tracing is explicit
because enabling a client event exporter or hook may require configuration and a
restart. Tell-Tail should report setup state, not silently rewrite client files.

Proposed CLI surface, subject to Phase 0:

```text
tell-tail mcp --trace-port 4318 --open
tell-tail mcp --trace-port 4318 --output json
tell-tail mcp --trace-replay ./synthetic-trace.ndjson --output json --once
```

`--trace-port` is appropriate only for an export-based adapter. A different chosen
adapter gets an explicit source option rather than an unused listener. A network
receiver cannot combine with `--once`; a finite replay can. Demo and replay modes
must not also read real client telemetry. Replay supports finite text/JSON output
and a browser view containing only imported fixture evidence. Live intake also
requires `--trace-token-file`; `--trace-config-out` writes an opt-in setup fragment.

For a receiver, use a separate loopback listener from the read-only dashboard.
It accepts only the selected protocol, method, route, and bounded encoding. Reject
browser-origin submissions, unexpected Host values, oversized bodies, unsupported
content types, and unsupported compression. Require a generated local credential
if supported by the selected exporter; keep it out of browser responses, URLs,
logs, command arguments, and the repository. Restrict any credential file to the
user. This does not defend against a malicious process already running as that user.

Validate/decode outside store locks, strip payloads, then enqueue normalized data.
Use protocol-correct acceptance, rejection, and retry responses. Never acknowledge
events as durably stored: v1 is an in-memory diagnostic collector. Handle export
retries without counting them as new tool calls. Saturation must have bounded work
and visible loss/rejection counters; it must not block plugin execution indefinitely.

Allowlist fields before storage, publication, replay export, or logging. Drop
arguments, result bodies/output snippets, prompts, account data, authorization
headers, URLs containing credentials, raw error strings, and raw conversation IDs.
Disabling prompt logging alone does not remove tool output from telemetry. Do not
log parser errors with their original body. Unknown records expose only bounded
format/version counters, not raw samples. Retain only fixed error categories and
source-defined safe codes. Payload opt-in is outside v1.

Setup documentation must show the exact client-specific configuration change,
restart requirements, a local verification call, disable/rollback steps, and how
to preserve an existing exporter. Do not silently replace an enterprise exporter;
where required, use an explicitly configured local collector to route events.
Hooks remain a separate candidate with their own trust/setup process, not a way
to bypass an unavailable telemetry option.

Keep the model, store, replay, and text/NDJSON code independent of the browser.
Initially gate an HTTP receiver behind the existing `web` networking feature,
while allowing it with `--output json` in default builds. No-default-features builds
retain passive/replay modes and reject the network option clearly. Revisit a
separate networking feature only if there is a concrete packaging need.

## Store, browser API, and pause behavior

Use a bounded current-call map plus an append-only ring of normalized revisions.
Each update has a monotonic ingestion sequence; each call has a stable local ID.
Source time is display data, not the pagination cursor. Make API IDs and counters
decimal strings or otherwise enforce JavaScript-safe bounds explicitly.

Proposed independent endpoints:

| Endpoint | Purpose |
| --- | --- |
| `GET /api/traces/snapshot` | Compact counts, capabilities, source status, revision watermark, omissions |
| `GET /api/traces/calls` | Filtered, bounded page; plugin/tool/status/execution/time filters and opaque cursor |
| `GET /api/traces/call/:id` | Safe call details and bounded provenance |
| `GET /api/traces/events` | Coalesced invalidation/current-watermark SSE, not an unbounded raw-event feed |

Queries use `as_of` ingestion watermarks and stable call ordering so that a row
completing does not move it between already-read pages. Bind cursors to collector
epoch, filter set, and watermark. Return an explicit expired-history response if
the evidence needed for that view has been evicted.

Pause must freeze existing rows and their outcomes, not just exclude new calls.
The current MCP `until` approach applies to immutable events; it cannot alone
freeze a mutable call record. Reconstruct the latest retained revision at or
before `as_of`, including inspection and filters, and expose the oldest supported
watermark. Never silently substitute current state into a paused view. A resume
loads the current watermark and source state; collection continues during pause.

NDJSON uses versioned `call_update`, `trace_source`, and `trace_gap` envelopes.
Consumers can materialize current calls by local ID and revision. Existing MCP
event/source envelopes keep their meanings and existing consumers remain valid
when tracing is disabled. Do not count two call revisions as two invocations.

## Dashboard experience

Add a **Tool calls** tab next to **Live timeline** and **Failure groups**. Default
to Tool calls when trace capture is explicitly enabled; keep Live timeline as the
passive-mode default. Do not auto-switch tabs when data arrives.

Columns: time, plugin/tool, status, duration, execution label, and Inspect. Running
rows update in place. Distinguish reported success from plain completion. Long
names wrap or truncate accessibly without widening the whole page. Keep a bounded
scroll region and sticky headers; preserve keyboard focus through updates.

Filters: plugin, tool, outcome, execution, time, and metadata search, with a clear
reset action. Query all retained calls on the backend, not just the current overview
page. Preserve each tab's filter state without implying that a lifecycle filter
also filters tool calls. Imported events never look like newly executed calls.

The inspector shows lifecycle observations, timing basis, attribution basis,
parent relationship when known, coverage gaps, and safe diagnostic categories.
Copy produces the same redacted metadata. No account balances or tool bodies are
needed to diagnose which action ran, how long it took, and its reported outcome.

Show trace capture status separately from the existing log watcher status:

- Disabled / waiting for exporter / receiving / stale or disconnected.
- Verified source/version and execution scope; lifecycle or result-only mode.
- Nested-call and subagent coverage: verified, partial, or unverified.
- Last received event, oldest retained call, evictions, unsupported events,
  rejected batches, and observable gaps.

No received events means waiting, not proof that no plugin calls occurred.
Do not display "100% captured" without an independent denominator. Keep metric
labels distinct: lifecycle observations, distinct tool calls, and transport records.

## Initial resource budgets

These are proposed tuning targets, not measured claims. Confirm them on the
reference machine and disclose changes in the validation record.

| Resource | Initial bound or target |
| --- | --- |
| Encoded request | 1 MiB hard limit, with request/header deadlines and bounded concurrency |
| Decoded event | 16 KiB normalized metadata; bounded field lengths, nesting, and attributes |
| Ingress queue | 1,024 events and 4 MiB, whichever is reached first |
| Current calls | 2,000 calls; at most 512 unmatched starts; combined metadata byte cap |
| Revision/dedup history | 8,000 revisions plus bounded dedup keys; 16 MiB total trace store budget |
| Source/plugin indexes | 64 sources, 256 plugin mappings, explicit overflow accounting |
| Browser page/detail | At most 100 rows / 64 KiB page; 16 KiB detail |
| Publication | At most five updates per second, coalesced; slow browsers cannot stall collection |
| Collector latency | p95 under 500 ms from receipt to rendered update, excluding exporter buffering |
| Resource target | Combined watcher under 128 MiB RSS; idle below 1% of one reference CPU core |
| Shutdown | Existing one-second cancellation target; bounded drain, incomplete calls labeled |

Bound raw decode memory and concurrent batch size as well as retained state.
Reference load: 100 calls/second for 60 seconds, including two revisions per call;
then a burst of 10,000 synthetic calls to verify bounded memory, fair service to
quiet sources, and truthful eviction/loss behavior. These are benchmark workloads,
not promises about arbitrarily high traffic.

## Implementation milestones and commit sequence

Each row is a coherent increment to verify and commit before the next independent
change. Split a row further when it grows; preserve the small commits in any PR.
No UI work should imply live desktop support before the capture audit establishes it.

| Order | Increment | Verification / exit condition |
| --- | --- | --- |
| 0a | Bounded capture probe and synthetic probe tests | No raw persistence; oversize/invalid data bounded |
| 0b | Installed-client capture audit and adapter decision | One real attributable Railway call; nested-call scope recorded |
| 1 | Trace envelope and sanitized fixtures | Exact decoding contract, nulls, versions, field budgets, sensitive sentinels |
| 2 | Identity and plugin resolution | Collisions, unknown identity, renamed catalog entries, multiple tools per plugin |
| 3 | Call correlation state machine | Concurrent/out-of-order calls, missing IDs, cancellation, conflicting outcomes |
| 4 | Bounded revisions, deduplication, and queries | Retry dedup, ID reuse across epochs, eviction, as-of consistency |
| 5 | Selected adapter decoder | Real verified shapes recreated synthetically; unknown format accounting |
| 6 | Receiver or selected source transport | Authentication/access rules, limits, malformed batches, overload and retry behavior |
| 7 | Runtime, shutdown, and source coverage | Collector failure isolation, queue saturation, source loss, passive watcher still works |
| 8 | Opt-in CLI, replay, and NDJSON | Flag conflicts, no-default-features, offline synthetic replay, output compatibility |
| 9 | Read-only trace API and SSE | Paging, frozen revisions, expired cursors, bounds, reconnect, slow consumers |
| 10 | Tool-call tab and compact rows | Empty/result-only/running/error states; keyboard and narrow-screen layout |
| 11 | Trace filters, inspector, and copy | Backend-wide filtering, tab state, pause/resume, redacted details and clipboard |
| 12 | Capture coverage UI and synthetic demo | Partial/nested-wrapper coverage, gaps, stale source, no false healthy state |
| 13 | Setup/rollback docs and package feature checks | Installed binary; supported source version and exact configuration instructions |
| 14 | Live acceptance and benchmark evidence | Real direct/nested calls on supported path, measured budgets, explicit remaining gaps |

Dependency order: audit -> contract -> correlator/store -> selected adapter/runtime
-> CLI/API -> UI -> installed/live acceptance. Synthetic contract and store work
can remain useful if a source is delayed, but does not satisfy desktop delivery.
Do not estimate the remaining integration work until milestone 0b resolves the
source and configuration path.

## Acceptance tests

Use table-driven Rust tests for state transitions and property tests for bounded
state and adversarial ordering. Run the actual binary for receiver, CLI, replay,
API, and browser acceptance. Keep fixtures synthetic and deterministic.

Required cases:

1. Two identical plugin/tool invocations remain two calls; retries of the same
   source delivery do not inflate counts. Parallel completions can arrive reversed.
2. Starts/results pair only within a proven scope. Restart, reused IDs, missing IDs,
   late arrivals, and disconnected sources cannot create false durations/outcomes.
3. Direct and nested calls are counted separately from wrappers, catalog discovery,
   and telemetry events. Parent relations appear only when the source supplies them.
4. MCP tool errors are not converted to success by successful transport delivery.
   Result-only sources do not fabricate starts, and denial is not tool execution.
5. Every outward surface rejects sensitive sentinel strings placed in arguments,
   output, errors, resource attributes, headers, and malformed input, including
   stderr, HTTP errors, NDJSON, snapshots, inspector, clipboard, and audit artifacts.
6. Saturation, invalid input, slow clients, source silence, and listener failure
   leave memory bounded and passive log collection usable. Loss is visible where
   observable; unseen upstream losses are never claimed to be measured.
7. Pause freezes row status, duration, inspection, and filtered history while calls
   continue to complete. Eviction produces an explicit expired view, not mixed time.
8. Filters search beyond the first 100 rows. Pagination does not repeat or skip
   calls because a row changed state. Reconnect honors collector epoch and gaps.
9. Browser checks at 1440, 768, 390, and 320 pixels cover keyboard tabs, focus
   preservation, modal dismissal, bounded scrolling, and no page-width overflow.
10. Disabling tracing restores passive behavior without client configuration damage.
    A stopped Tell-Tail process must not prevent normal plugin execution on the
    verified setup; document any measured exporter delay or hook timeout behavior.

Run the existing sequential Rust feature matrix, Clippy, browser suite, and package
checks from [dashboard testing](TESTING-DASHBOARDS.md) and
[MCP validation](VALIDATION-MCP.md). Rebuild the intended binary after feature
matrix tests. Do not report skipped native visibility tests as passed. Benchmark
timing belongs in explicit local validation, not flaky shared-runner assertions.

Before declaring the feature complete, demonstrate a known series of real,
read-only plugin calls on the target desktop path. Record expected vs observed
calls, attribution, nested execution, outcomes available from the source, version,
configuration scope, and measured export delay. Retain sanitized counts and shape
evidence only. A synthetic demo or a successful CLI trace alone is insufficient.

The two 24-hour stdin/NDJSON and followed-file/metrics soaks remain optional,
non-blocking background validation. They never gate CI, release, publishing, or
deployment and require no per-release waiver. Preserve results and investigate
actionable failures separately. Any additional extended tracing soak is also
optional unless the user explicitly requests otherwise.

## Completion criteria and open decisions

Complete the initial feature when one supported desktop capture path has verified
per-plugin call coverage, the bounded collector and dashboard meet the acceptance
checks, installed-binary setup/rollback is documented, and unverified paths are
clearly identified. Result-only support is a useful explicitly scoped delivery;
it does not fulfill a claim of real-time start/completion coverage.

Resolve these through evidence, in order:

1. Which mechanism sees hosted plugin calls and nested executions in this app?
2. Does it expose identity and stable call IDs, or require a verified catalog map?
3. Does it report both lifecycle ends and actual tool errors, or only execution results?
4. Is capture per chat, process, local host, or workspace, and how are new chats covered?
5. What setup, restart, policy, and existing-exporter constraints apply?
6. Which transport and dependency versions fit the selected source and Rust baseline?

Do not add account access, cloud infrastructure, a proxy, or broad configuration
changes just to avoid answering these questions. The first useful proof remains
one actual plugin call appearing as one correctly attributed trace.

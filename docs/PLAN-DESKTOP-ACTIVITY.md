# Tell-Tail for ChatGPT and Codex implementation plan

Status: implemented in the source checkout, November 19, 2026. See the
[usage and API guide](DESKTOP-ACTIVITY.md) and
[validation record](VALIDATION-DESKTOP-ACTIVITY.md). The audited adapter,
unified workspace, owned service migration and live capture check are complete.
Cross-source chat correlation and complete cloud visibility remain unverified;
the UI exposes those limits. Tool projection samples bounded accepted revisions
and reports coalescing/loss instead of introducing a second queue.

This plan follows the working plugin tracing and collector lifecycle implementation. It does not authorize or
require beta packaging, a release, a version bump, or new cloud infrastructure.

Build a local observability workspace that answers: what happened, which tool or
desktop component was involved, what failed, and what evidence supports the next
step. A unified activity timeline is the main experience. MCP remains a first-class
diagnostic domain alongside plugin calls, connections, and desktop runtime events.

The UI is a core deliverable with explicit acceptance criteria, not a final styling
pass. Prototype the interaction and visual hierarchy before connecting live data.

## 1. Product decisions

- Product name: **Tell-Tail for ChatGPT and Codex**. Keep the Tell-Tail mark and
  existing local, offline-capable engine. Use precise source labels inside the UI.
- Default destination: **Activity**, even when tool tracing is enabled. Never
  switch destinations because an event arrives.
- Primary navigation: **Activity**, **Tools & MCP**, **Problems**, **Sources**.
  Chat context is a filter and inspector section initially. Add a standalone Chats
  destination only after source identity and useful chat coverage are verified.
- One local collector, one browser workspace, multiple adapters. No second file
  follower for the same logs and no browser-side joining of independent live feeds.
- Automatic discovery reads supported log roots. Existing explicit setup continues
  to govern exporter configuration; passive discovery must not enable telemetry.
- Open the browser on explicit Open/Setup or `--open`. Login/background startup
  stays quiet. A startup process must never repeatedly open tabs on restart.
- Default capture remains metadata-only and bounded in memory. Persistent capture
  after login does not mean persistent event history.

The product name describes the focus, not a promise of complete cloud visibility.
The first implementation observes supported desktop paths. Do not label local
tool results as a complete trace of ChatGPT cloud execution or reasoning.

## 2. Evidence and initial source scope

The November 19 local inspection found ChatGPT connection events, app-server
responses, plugin install/sync activity, browser startup, and local runtime
components in the same logs that Tell-Tail already follows. That inspection
established event names and field presence; it did not establish the semantics of
every field, a complete call lifecycle, or cross-source identity equivalence.

The documented desktop log root is
`~/Library/Logs/com.openai.codex/YYYY/MM/DD/`. The retained directory name is not
sufficient to determine the product surface of every record. [Official desktop
troubleshooting](https://learn.chatgpt.com/docs/reference/troubleshooting.md)

| Source family | First useful events | Inclusion condition |
| --- | --- | --- |
| Existing MCP diagnostics | Startup, ready, failures, authentication/discovery, connection lifecycle | Preserve current verified semantics and mirror handling |
| Existing tool telemetry | Attributed plugin/tool results, reported duration and execution outcome | Preserve result-only and coverage labels; no fabricated starts |
| ChatGPT connection logs | Transport opened/closed, reconnect scheduled, topic subscription | Audit scope and close semantics; closure alone is neutral |
| Plugin lifecycle logs | Install succeeded, marketplace add/sync completed, recognized failure shapes | Allowlist identifiers and codes; omit marketplace URLs and paths |
| Browser/runtime logs | Recognized startup started/ready and dependency diagnose events | Pair only with verified identity; readiness is a last observation |
| App-server responses | Recognized response methods, duration, explicit error codes | Method allowlist; unknown errorCode values are not automatically failures |
| Local executor/worktree components | Specific verified lifecycle events | Candidate increment; a component name alone is insufficient |
| Chat/view events | Stream role/activity/subscription changes | UI events describe the view, not model execution or chat completion |

Candidate later telemetry includes model requests, stream events, approval
decisions, and token usage. Official configuration documentation describes these
signals, but the installed-client path and each field's meaning must be verified
before enabling an adapter. Local configuration is not a guarantee of Work Cloud
coverage. [Official telemetry configuration](https://learn.chatgpt.com/docs/config-file/config-advanced.md)

Defer standalone ChatGPT app discovery if its source has not been verified, Claude
general-event expansion, token/cost dashboards, CPU/memory sampling, prompt/output
capture, cloud history, automatic repairs, notifications, and LLM explanations.
Preserve existing Claude MCP support and the generic file/stdin dashboard.

### Source audit deliverable

Create a bounded metadata inventory and synthetic fixtures before broad parsing:

1. Record app and engine versions, surface tested, event names, field types,
   timestamp basis, identity scope, and event semantics.
2. Observe normal startup, normal idle, one existing plugin call, and natural
   connection/runtime events. Do not force outages or reinstall plugins merely
   to create samples; use synthetic failure cases where necessary.
3. Audit whether response duration describes routing, transport, or execution.
   Until established, label it **reported duration**, not model/tool latency.
4. Recreate shapes with synthetic labels and identifiers. Never check in copied
   log lines, prompts, account values, browser addresses, or private paths.
5. Produce an explicit capabilities matrix: verified, partial, unsupported, and
   unverified, scoped by client surface/version and event family.

## 3. Visual direction

### Composition

Use an instrument-panel layout with restrained chrome: strong typography, a shared
time axis, quiet surfaces, compact rows, and a clear selected-event detail pane.
Visual emphasis comes from alignment and meaningful event patterns. Avoid a grid
of metric cards, decorative charts, oversized status numbers, or a large hero area.

At 1440 × 900:

- A 56 px application header holds the mark, product descriptor, collector
  connection state, Pause display, and settings/appearance controls.
- A 184–208 px left rail contains the four destinations and compact source status.
- The center owns the remaining space. Its header and filters occupy approximately
  100 px; a 96–112 px time navigator follows; activity rows begin within 300 px
  of the viewport top.
- The event feed is the main scrolling region with sticky time/filter context.
- Selection opens a 320–360 px right inspector without pushing the feed downward.
  It has its own scroll region and a visible close control. Closing restores focus.
- No permanently expanded source cards sit above the timeline. Coverage belongs
  in a small status line and the Sources destination.

At 1280 × 800, with the inspector open, show at least seven meaningful event rows
without page scrolling. At widths under 1100 px the rail becomes compact navigation
and the inspector becomes a dismissible overlay. Below 760 px use one content
column, a full-width detail sheet, and wrapping filters. At 320 px there must be no
page-width overflow or loss of essential event meaning. Short windows can use page
scrolling instead of trapping the user in tiny nested scroll regions.

### Visual language

Extend [the current design system](DESIGN.md), preserving the upright-hook mark:

- Dark: graphite canvas `#171C19`, raised surface `#202722`, warm text `#F0F1E9`.
- Light: warm canvas `#F7F6F2`, white surfaces, ink `#222B25`.
- Green marks selection/affirmative outcomes with readable light-theme pine.
  Violet marks connectivity/information, amber warnings, coral explicit failures.
  Neutral gray communicates unknown, incomplete, and unavailable values.
- Use category icons and labels, not a rainbow for every plugin. Color never
  carries meaning alone. Keep large surfaces quiet; severity accents stay local.
- System sans-serif; monospace only for times, durations, and technical identifiers.
  Main heading 24–28 px, event summary 14 px, supporting metadata 12–13 px.
  Use tabular numerals. Do not shrink essential text to make columns fit.
- A 4 px spacing grid, 6–8 px controls, thin separators, and minimal shadows.
  Comfortable rows approximately 52 px; compact mode approximately 40 px.
- Short, purposeful transitions only: selection and inspector changes in 120–180
  ms. No looping pulse, continuous shimmer, confetti, or animated feed insertion.
  Honor reduced motion. Live activity should not visually shake the page.

Both themes receive intentional contrast checks. The product must feel finished
in light mode as well as dark mode; all assets and fonts remain available offline.

### Time navigator

A shared horizontal time axis provides a compact map of the selected interval:
Tool calls, MCP, Connections, and Desktop lanes. Marks show observed event times;
they are not spans unless a start/end pair is supported. Dense data becomes bins
with exact counts; sparse data stays discrete. Clicking a mark/bin narrows the
feed. Drag selection is an enhancement, with equivalent keyboard time controls.

Show unknown-time records in a separate **Time unavailable** group. Never squeeze
them into the first time bucket. Label the displayed timezone. Imported history
and known collection gaps receive distinct annotations; silence alone is neither
a gap nor an outage. Hide the navigator when no meaningful time range exists.

Begin with accessible SVG and an exact-values alternative. Do not add a heavy
charting dependency or canvas renderer until profiling justifies it.

### Event rows

Each row answers a small set of questions in a stable order:

`time | category icon | plain-language event + subject | scope | outcome/duration`

Example: `14:32:08 · Connections · ChatGPT reconnect scheduled · App-wide · Attempt 2`.
An icon and small colored marker distinguish failures without making the entire
row red. A selected row gets a quiet fill and a strong leading selection mark.
Use generated summaries from recognized fields, not raw log text.

Long tool names truncate visually with an accessible full name. Secondary metadata
can wrap in comfortable mode; it must not make every row a miniature card. Unknown
duration is an em dash with accessible explanation, never zero milliseconds.

### Filters and density

- Always visible: metadata search, time range, category chips, and active filter
  summary. Category chips include **MCP** and **Tools** separately.
- More filters: surface, source, plugin, server, chat, outcome, historical/live,
  and technical detail level. These are backend filters over all retained data.
- Default **Essential** hides routine response routing and other known noisy
  diagnostics, with a visible hidden-event count and **Show diagnostics** control.
  Failures, warnings, lifecycle transitions, and tool results remain visible.
- Comfortable/compact changes presentation only. It does not change the meaning
  of counts, hide errors, or silently change retention.
- Consecutive equivalent diagnostics can fold into an expandable row. Group only
  identical safe kind/subject/scope/code within a fixed short window, retaining
  occurrence counts and bounded evidence. Never fold distinct tool invocations.

### Inspector

Order detail as: what was observed; explicit outcome; subject and scope; timing;
related evidence; provenance and coverage; copy sanitized diagnostic summary.
Keep the readable summary above implementation fields.

Separate **Linked evidence** (verified shared identity) from **Nearby in time**.
Nearby records never appear as children or causal links. Only recognized failure
codes produce deterministic suggested checks. There is no generic AI diagnosis
button or in-browser repair action in this increment.

## 4. Interaction and state specification

| Situation | Required behavior |
| --- | --- |
| Live feed at the latest position | Coalesced updates, stable row identity; no animated reorder |
| User scrolls into history or selects evidence | Hold the visible window and selection; show an explicit new-activity button |
| Pause display | Freeze rows, revisions, chart, counts, ages, filters, and inspector at one watermark; capture continues |
| Resume | Refresh to a new watermark explicitly; preserve filter choices |
| Source event arrives late | Mark delayed arrival; do not move already inspected rows under the pointer |
| Retention expires a frozen view | Explain expired history and offer return to live; do not substitute current data |
| Collector reconnects with same epoch | Refresh at a consistent watermark and disclose observable loss |
| Collector restarts | Explain history reset; clear incompatible cursor/selection, preserve safe filters |
| No matching filters | Show active filters and Clear filters, not setup instructions |
| No logs found | Show checked source locations and keep discovery active |
| Exporter disabled or waiting | Passive events remain useful; explain tool-capture status separately |
| No recent events | Show last observation and source state; do not infer healthy, failed, or thinking |
| Source unreadable or format changed | Other sources continue; show a scoped source problem and coverage impact |
| Only imported history | Label history explicitly; no misleading Live activity claim |

Keyboard support: native tab order, visible focus, Enter to inspect, Escape to
close, and focus restoration. Optional list navigation must not intercept typing
in search. Announce connection/selection changes politely, not every incoming row.
Ensure 44 px touch targets on coarse pointers and readable 200% browser zoom.

The companion [interactive concept](design/desktop-activity-concept.html) is a
synthetic design study. It demonstrates filtering, selection, and evidence layout;
it is not connected to the collector and does not establish live coverage.

Prototype validation on November 19: inspected dark/light screenshots; exercised
category filters, search, event selection, Problems and Sources with Playwright.
No script errors or horizontal overflow at 1024, 768, 390 and 320 px content widths.
At 1280 px content width, the first row begins at 298 px and nine rows fit within
800 px. This validates the concept's geometry and basic interactions, not the
production pause/reconnect behavior or the complete accessibility requirements.

## 5. Domain model and interpretation

Add `src/activity/`; do not broaden the required MCP `server` field to represent
all desktop events. Keep domain-specific call lifecycle and MCP issue logic.

Proposed versioned normalized activity contract:

| Field | Meaning |
| --- | --- |
| schema_version, collector_epoch, sequence | Activity contract version and local ingestion order; IDs/counters serialize safely for JavaScript |
| item_id, revision, observation_id | Stable display item, its update, and the underlying observation; repeated calls remain distinct |
| source_id, source_generation, adapter_version | Provenance and parser capability scope |
| product_surface | Verified surface or unknown; never inferred from the directory name alone |
| occurred_at, received_at, time_basis | Optional source time, receipt time, and timestamp interpretation |
| category, kind, severity | Separate event family, semantic kind, and explicit severity |
| subject | Typed tool/plugin/server/runtime/connection label; optional safe metadata |
| scope | App-wide, process, chat, turn, or unknown, with opaque local references |
| outcome, duration, duration_basis | Optional source-supported values; completion and execution success stay distinct |
| summary_key, safe_fields | Template-generated text from typed allowlisted fields; no arbitrary payload map |
| origin_ref, provenance | Domain item/revision reference and bounded file-generation/offset locators |
| historical, coverage_flags, correlation_basis | Completeness and interpretation limits |

Use separate types for raw adapter input, sanitized internal events, and public
responses. Raw records never implement general serialization and never enter
error strings. Enum fields do not accept arbitrary strings as fallback text.

### Identity and ordering

- One process-local identity context for new activity correlations. The existing
  MCP and trace stores use independent hashing contexts; equal-looking labels
  cannot currently be joined. Introduce a shared resolver deliberately.
- Only map cross-source IDs after an audit demonstrates equal semantics and scope.
  A thread ID, conversation ID, request ID, and execution ID are not interchangeable.
- Key request identity by verified client/source execution epoch as well as ID.
  A file rotation is not automatically a process restart; timestamps alone never
  bridge epochs. Unknown scope stays unknown.
- Use opaque run-local labels such as Chat A; do not read conversation databases
  for titles. Titles and prompts are outside the initial capture scope.
- Sequence is the pagination/freeze authority. Default feed order follows
  ingestion sequence, with source time displayed and delayed arrivals marked.
  The navigator uses source event time; selecting a range returns matching records
  in stable ingestion order. This distinction is visible in help text.
- A tool call is one activity item with revisions. New lifecycle evidence updates
  that item; a second invocation is another item. MCP observations stay distinct.
- Cross-source deduplication requires a common verified identity. Otherwise show
  separate observations and do not inflate a tool invocation count with log events.

Counts must name their unit: activity items, observations, tool calls, or grouped
problems. Time bins count matching activity items by their fixed anchor timestamp;
call updates do not add a second call. Folded rows display their occurrence count.
Frozen queries include only revisions and group members known at their watermark.

## 6. Collector architecture and file changes

```text
existing bounded discovery + file follower
  -> desktop parser dispatcher -> sanitized desktop observations --+
  -> existing MCP parser/store -> sanitized MCP projection --------+--> activity store
existing OTLP receiver -> existing call store -> call revisions ---+       |
                                                                         v
                                             versioned read-only API / SSE / NDJSON
                                                                         |
                                                  activity UI + domain inspectors
```

| Location | Planned change |
| --- | --- |
| `src/activity/model.rs` | Typed events, capabilities, public schemas, limits |
| `src/activity/parse/desktop.rs` | Audited event-name/field allowlists, version handling |
| `src/activity/identity.rs` | Shared opaque identity context with explicit scope rules |
| `src/activity/project.rs` | Map accepted MCP observations and call revisions into activity items |
| `src/activity/store.rs` | Bounded revision history, indexes, groups, bins, consistent queries |
| `src/activity/runtime.rs` | Shared source orchestration and bounded projection delivery |
| `src/activity/cli.rs`, `src/main.rs` | New desktop command, replay, output, compatibility routing |
| `src/mcp/collection.rs` | Reuse follower/framing/provenance; extract neutral primitives only as needed |
| `src/mcp/store.rs` | Expose sanitized accepted/deduplicated updates without re-counting mirror records |
| `src/tool_trace/store.rs`, `receiver.rs` | Expose accepted call revisions and safe correlation metadata |
| `src/web/activity.rs`, `assets.rs` | Add activity API/router and self-contained assets |
| `web/activity.html`, `.css`, `.mjs` | New shell and state controller |
| `web/activity-feed.mjs`, `activity-time.mjs`, `activity-detail.mjs` | Focused rendering modules sharing one view state |
| `src/tool_trace/service.rs`, lifecycle helper | Explicit managed-service migration once the new command is validated |
| `tests/activity_*.rs`, `browser-tests/activity*.test.mjs` | Parser/API/CLI/integration and browser behavior |

Initially keep existing MCP/trace stores authoritative for their domains. Feed a
sanitized, compact projection into the activity store after domain acceptance.
Adapters return deltas outside domain locks; do not hold two store locks together.
Use a bounded projection queue with sequence and loss accounting. If it fills,
retain domain ingestion behavior, report activity projection gaps, and never
silently pretend the unified timeline is complete. This also prevents browser
work from blocking telemetry intake or file following.

The activity inspector reads its own retained revision at the requested watermark.
It must not fetch a newer domain record from an old frozen view. Additional domain
evidence is retrieved only by an exact retained revision; otherwise label it
unavailable. Source status and problem summaries need versioned snapshots too.

Retain domain-specific security and parser limits. Refactor common collection
utilities only where needed; avoid a wholesale rename of `src/mcp/` as a first step.

## 7. Read-only API and view state

Add a separately versioned `/api/activity/` contract; existing `/api/mcp/` and
`/api/traces/` routes keep their meanings.

| Route | Purpose |
| --- | --- |
| `GET /api/activity/snapshot` | Epoch, watermark, retention boundary, counts, capabilities, source status |
| `GET /api/activity/items` | Filtered page, stable ordering, bounded opaque cursor |
| `GET /api/activity/item/{id}` | Exact item revision and safe evidence as of watermark |
| `GET /api/activity/bins` | Bounded time buckets with the same filters and watermark |
| `GET /api/activity/problems` | Scoped failure groups with affirmative recovery evidence |
| `GET /api/activity/sources` | Detailed capability, freshness, format and omission information |
| `GET /api/activity/events` | Coalesced SSE invalidation containing epoch/watermark, not full event history |

Bind cursors to epoch, watermark, normalized filters, ordering, and schema. Enforce
bounded parameters and reject unknown/duplicate query fields. Give expired history
an explicit response rather than an empty successful page. Create snapshot tokens
under one activity-store lock so rows, bins, counts, and inspection agree.

Client state is `{epoch, watermark, destination, filters, cursor, selection,
displayMode, pendingWatermark}`. Abort outdated requests and ignore results from
older navigation generations. Preserve selection by item ID, not row position.
Use one API state controller rather than separately polling each UI section.

Persist only appearance/density and non-sensitive preferences locally. Do not
persist search text or event contents. Treat URL state as optional non-sensitive
filters; no raw chat identifiers or credentials in links, history, or storage.
Retain loopback binding, Host/Origin checks, CSP, text-only rendering, bounded
responses, and slow-client isolation. No write/repair routes are added.

## 8. Resource and privacy budgets

These are initial engineering targets to measure, not claims about current code.

| Resource | Proposed bound or target |
| --- | --- |
| File discovery/history/record framing | Preserve current 64-source, 16 MiB bootstrap, 256 KiB record limits |
| Desktop normalized observation | 4 KiB maximum; bounded strings and provenance, no raw body |
| Activity projection queue | 1,024 updates or 4 MiB, whichever comes first |
| Activity store including indexes/revisions/groups | 24 MiB hard budget; at most 8,000 revisions and 4,000 items |
| Chat/subject/group indexes | At most 1,024 entries per index under the shared byte budget |
| Snapshot/page/detail | 64 KiB / 100 rows and 64 KiB / 16 KiB |
| Navigator | At most 120 buckets per lane, bounded lane count |
| Publication | At most five coalesced invalidations per second |
| Browser | At most 100 mounted feed rows; no unbounded retained pages |
| End-to-end latency | p95 under 500 ms after receipt/completed log write, excluding host buffering |
| First usable view | Under 2 seconds on the reference 32-source/16 MiB bootstrap workload |
| Combined process | Target under 128 MiB RSS; idle below 1% of one Apple Silicon core |

The activity budget is additional to existing domain stores. Benchmark total
process memory and parser/encoding transients; lower budgets or remove duplicate
retention if the combined target is missed. Cap dedup caches and unknown-event
counters. Noisy diagnostic families must not evict all useful tool/MCP evidence:
reserve bounded category capacity and expose omission counts when limits apply.

Before any retention, drop prompts, outputs, arguments, URLs, paths, account data,
authorization material, arbitrary error messages, and raw chat/request IDs. Store
only recognized field values with size/type constraints and generated summaries.
For unknown shapes retain bounded counters, not raw samples. Diagnostic copying
uses exactly the same sanitized representation as the inspector. No automatic
uploads, background inference, or persistent raw-log duplication.

## 9. Compatibility and managed setup

Proposed public entry point: `tell-tail desktop --open`; also support text/JSON,
explicit source directories, and synthetic `desktop --demo --open` without reading
real user logs. Reserve the positional name `desktop` consistently with `mcp` and
`tracing`; a file named desktop remains accessible via `./desktop` or `-- desktop`.

Keep `tell-tail mcp` and its schemas working. Preserve the existing dashboard until
the new workspace passes acceptance; offer an explicit link during development.
Existing tool tracing remains opt-in and works with the old command. In no-web
builds, keep passive desktop text/replay usable and reject web/receiver flags clearly.

Managed service migration is a dedicated increment, not incidental to UI delivery:

1. Verify the new desktop command supports existing stable ports, credentials, and
   source options before changing a service definition.
2. Version install metadata and recognize the exact old owned plist. The current
   upgrade implementation rejects modified plists; add an explicit migration for
   known owned definitions while continuing to reject user-edited definitions.
3. Back up executable, metadata, and plist together; stage the new command; restart
   and validate collector health. Failure restores all three as a consistent set.
4. Keep exporter endpoint/key unchanged. Restore the old command/metadata/plist
   during explicit rollback. Verify disable/uninstall against both generations.
5. Update plugin setup/open guidance to land on Activity. No browser launch at login,
   no silent exporter reconfiguration, no installation changes during planning.

## 10. Small implementation increments

Commit each coherent verified increment before the next independent one. Preserve
the commits when pushing. Split any row below further when necessary.

| Order | Increment | Exit evidence |
| --- | --- | --- |
| 0a | Metadata inventory and capability matrix | Event names, types, timing, scopes, and versions recorded without raw content |
| 0b | Synthetic normal/failure/dense/empty fixtures | Unknown fields, misleading errors, and sensitive sentinels covered |
| 1a | Visual shell and feed prototype | Desktop/light/dark/narrow layouts; timeline visible immediately |
| 1b | Inspector, filters, freeze and state prototypes | Core diagnostic journey usable with keyboard and realistic fixture density |
| 2a | Typed activity model and sanitizer | Schema, size, enum and metadata boundaries tested |
| 2b | Shared identity and correlation contract | Scope collisions, missing IDs, restarts, and no proximity-based joins |
| 3a | Connection and plugin lifecycle adapter | Golden fixtures, normal close handling, unknown-format accounting |
| 3b | Browser/runtime and response adapter | Verified event subset and method allowlist; high-volume diagnostics separated |
| 4a | MCP and tool-call activity projections | Mirror dedup preserved; one item per call; revision semantics intact |
| 4b | Activity retention and frozen queries | Eviction, loss, indexing, bins, filters and as-of consistency |
| 4c | Source status and scoped problems | Readiness/freshness separated; recovery requires matching affirmative evidence |
| 5a | Desktop runtime and CLI | One follower per source, replay isolation, feature and filename compatibility |
| 5b | Activity API and SSE | Read-only security, cursors, freeze/expiry, reconnect and bounded serialization |
| 6a | Production Activity shell/feed | Real API state controller; stable selection, pending-update control |
| 6b | Time navigator and filters | Time/ingestion semantics, bin parity, accessible range selection |
| 6c | Tools & MCP, Problems, Sources | Existing domain diagnostics remain accessible; coverage is actionable |
| 6d | Theme/responsive/accessibility polish | Visual screenshots plus measured layout and keyboard acceptance |
| 7a | Owned managed-service migration | Native isolated upgrade/rollback/uninstall across command generations |
| 7b | Plugin guidance and developer docs | Accurate setup/open/coverage behavior; self-contained demo |
| 8 | Live validation and performance record | Real observed desktop events plus tool results; limits and gaps reported |

Dependency path: audit -> model/identity -> adapters/projections -> store/runtime
-> API -> live UI -> service migration -> live acceptance. Prototype work can
precede backend work, but a synthetic interface is not evidence of adapter support.

The first vertical slice is one real connection lifecycle event, one existing MCP
event, and one tool result in the same timeline, each inspectable with truthful
scope and provenance. It must work before expanding the event catalog.

## 11. Verification and completion criteria

### Data and collector acceptance

- Rotate/truncate logs, add a new dated directory, interleave sources, split lines,
  exceed limits, and simulate read failures without duplicating followers/events.
- Test quoted structured fields, malformed records, unknown versions, timestamps
  missing timezone, clock reversal, imported history, and late delivery.
- Test missing IDs, identical IDs across epochs, different IDs close in time,
  duplicates, conflicting outcomes, and unavailable related evidence.
- Test every outward surface with secret/prompt/path/URL sentinels, including
  parser errors, stderr, API, SSE, NDJSON, inspector, copy, and saved fixtures.
- Freeze while calls finish and groups change; verify rows, counts, bins and
  inspection stay consistent. Expired snapshots must fail explicitly.
- Stress 100 observations/sec for 60 seconds, then a 10,000-event burst including
  one quiet source. Measure memory, publication lag, fairness and omissions.
- Preserve existing MCP/trace/CLI regressions and run the repository's sequential
  Rust feature matrix and Clippy. Rebuild the intended binary before browser tests.

### UI acceptance is part of delivery

- Review screenshots at 1440×900, 1280×800, 1024×768, 768×1024, 390×844 and 320×640,
  in both themes. Cover empty, sparse, dense, failure, paused, disconnected and
  expired-history states, not just a beautiful populated screen.
- At 1280×800, show at least seven event rows with the inspector open. No source
  summary or metric-card wall may push activity below the initial viewport.
- Search/filter the complete retained history. Verify matches beyond the first
  100 rows and count/bin agreement with the selected filter and watermark.
- Live updates preserve keyboard focus, selection, scroll anchor and inspector.
  Long labels, 200% zoom, reduced motion and touch layouts remain usable.
- Meet WCAG AA contrast targets: 4.5:1 normal text and 3:1 large text/essential
  non-text indicators; verify status without color and an exact-values chart view.
- A user can find a failed call, inspect evidence, locate related MCP/connection
  observations, and copy a safe summary without leaving the workspace.
- A normal close, silent source, successful transport, or unknown duration must
  not produce a false failure, healthy status, tool success, or zero duration.
- Browser tests cover behavior; screenshot review establishes composition and
  polish. Passing DOM assertions alone does not satisfy the design requirement.

Live acceptance records the installed app/engine versions, expected and observed
events, coverage limitations, receipt delay, and resource results. Cloud-path and
chat correlation claims remain unverified unless independently demonstrated.

The existing two 24-hour soak scenarios stay optional, non-blocking background
validation. They never delay CI, publishing or deployment. This work introduces
no beta packaging or release gate.

## 12. Decisions to resolve through evidence

1. Which event fields prove chat identity across logs and tool telemetry? Until
   answered, show app-wide/unknown scope rather than manufactured chat groupings.
2. Which connection and runtime records distinguish expected closure, failure,
   and affirmative recovery? Generic closure stays informational.
3. Does response duration measure the user-visible operation? Until verified,
   keep the source's label and avoid model-latency claims.
4. Which product surfaces emit the audited events? Branding stays broad while
   the Sources view names the verified scope precisely.
5. Does the combined memory/DOM budget hold under real dense logs? Tune retention
   and diagnostic visibility using measurements, preserving clear omissions.

These questions constrain individual adapters or views; they do not block the
useful unified desktop/MCP/tool timeline or justify expanding into release work.

# Desktop activity implementation validation

November 19, 2026 · macOS arm64 · source branch `codex/plugin-tool-tracing`.
Behavior and UI changes through `9b612ae`; plugin discovery wording through
`121a31a`. This validates source implementation and the local managed collector.
No beta archive, release package, version bump or publication was produced.

## Implemented and checked

- One existing follower feeds the general desktop adapter and MCP domain. The
  tool projection preserves qualified outcomes and discloses missed/coalesced
  revisions. Unknown event bodies and sensitive fixture sentinels do not enter
  the activity store, API, headless output, inspector or copy summary.
- Activity, Tools & MCP, Problems and Sources share a watermark for rows, bins,
  source state and inspection. Backend queries cover retained evidence beyond
  the first page. Late updates, conflicting outcomes, explicit expiry and epoch
  changes preserve reading state until returning to live.
- Existing MCP commands, APIs and dashboard remain available. Literal `desktop`
  filenames and headless builds are covered. Demo isolates synthetic sources
  and includes all four activity categories.
- Native isolated launchd testing exercised legacy MCP metadata without a
  command field, migration to desktop, rollback to MCP, failed executable
  activation/recovery, edited plist/exporter refusal, disabled upgrade and
  uninstall including interrupted cleanup. Credential and config bytes stayed
  unchanged. The native test ran explicitly rather than remaining skipped.

The sequential Rust matrix passed `cargo test` and `cargo clippy --all-targets
-- -D warnings` with default features, `--no-default-features`, and
`--no-default-features --features snapshot-notifications`; formatting passed.
The default binary was rebuilt before browser verification.

The final browser suite passed **40 tests**. One existing native hidden-tab test
was skipped in headless mode. The four Activity browser scenarios cover actual
log following and authenticated protobuf intake, all-history search and paging,
frozen outcomes, recovery groups, source coverage, clipboard privacy, responsive
inspection, keyboard focus, exact times, expiry, disconnect and collector restart.

## Visual and interaction review

The embedded production UI was reviewed using synthetic data at 1440×900,
1280×800, 1024×768, 768×1024, 390×844 and 320×640 in both themes, plus Problems,
Sources and the side inspector. The [retained screenshot](design/desktop-activity-implemented.png)
shows the actual implementation, not the earlier concept.

At 1280×800 the first row is at or above 300px, with at least seven rows visible
while inspecting an event. Palette contrast minima are 4.86:1 for normal text
and 3.14:1 for control outlines in light mode; 5.63:1 and 3.89:1 in dark mode.
These palette checks do not constitute a complete accessibility audit.

Screenshot review caught a tablet grid placing content below the sidebar even
though horizontal-overflow tests passed. The corrected layout has geometry
regressions at 1024, 1100 and 1200px. Touch checks enforce 44px navigation,
category, header and Exact times controls. Reduced-motion and modal inspection
are tested; a 640px CSS viewport checks the reflow required at 200% on a 1280px
display. A full screen-reader audit and browser-menu zoom test were not performed.
One loaded-machine rerun hit the helper's former three-second startup timeout;
the bounded startup allowance is now ten seconds. Final full suite passed.

[UI evidence](validation/desktop-activity-ui-2026-11-19.json) records dimensions,
contrast and reviewed screenshots. The synthetic screens contain no user logs.

## Finite mixed-load run

[Load evidence](validation/desktop-activity-load-2026-11-19.json) comes from
`scripts/activity_benchmark.py` against the optimized source executable:

| Measurement | Observed |
| --- | --- |
| Bootstrap | 16 MiB in 32 history files plus one active source; drain ~1.04 s |
| Steady input | 4,800 tool results + 1,200 log records in 62.612 s |
| Target / achieved rate | 100 / ~95.8 observations per second, including query waits |
| Tool post to Activity query, p95 | 111.229 ms |
| Connection burst | 10,000 records drained in 1.241 s |
| Peak sampled collector RSS | 17,200 KiB |
| Idle collector CPU | 0.2% over five seconds |
| Backpressure retries / projection gaps | 0 / 0 |
| Quiet MCP evidence | Retained through the connection flood |
| Shutdown | 237.602 ms |

Activity and trace retention/charged-byte assertions passed. This is one local
run on a shared machine while the Rust regression matrix also ran, not a platform
capacity guarantee. RSS excludes browser memory. Query visibility is measured;
paint latency and per-browser memory were not measured. The two 24-hour soaks
remain optional, non-blocking, and were not run for this change.

## Live installed collector

The existing private login service was upgraded from `mcp` to `desktop`, then
refreshed with the final responsive UI correction. Client config, credential and
owned exporter bytes were verified unchanged. Both receiver and activity health
passed. Codex stayed running and no exporter restart was needed.

The new collector followed 59 local files. Its retained observations included
MCP, connections and general desktop events. A harmless GitHub `_get_user_login`
call appeared as a tool result in the unified API; the final build also rendered
that result in Activity and opened its reported-success evidence in the inspector.
[Live evidence](validation/desktop-activity-live-2026-11-19.json) records the
installed ChatGPT bundle version, supported metadata and coverage limits without
saving native chat IDs, payloads or private paths. The current engine version was
not independently queried; prior engine verification remains documented separately.

This live check uses bounded imported history plus a newly exported tool result;
it does not claim that every desktop lifecycle transition was triggered during
the check. Fresh Desktop plugin installation was not repeated or repackaged.
Result-only semantics, missing starts/parents, unverified cross-source chat
identity and incomplete cloud-path coverage remain explicit product limits.

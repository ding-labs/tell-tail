# Unified workspace validation

November 19, 2026 · macOS arm64 · `codex/plugin-tool-tracing` · implementation
through `5165e45`. This validates the source build and existing local collector;
no beta package, release, version bump or fresh plugin installation was produced.

## Behavior and regression checks

- File/stdin launches still open the original Logs dashboard. Its charts, severity
  counts, parsing guidance, recent errors and ingestion diagnostics remain available.
  Switching to the AI entry page leaves the original stream collecting and does
  not start desktop discovery or make activity API requests.
- Desktop launches expose Logs and AI Activity through the same collector.
  Complete followed records feed bounded aggregate metrics; private raw bodies
  do not enter its snapshots or an error-preview buffer. Desktop counting and
  history semantics are explained in the UI and [workspace contract](WORKSPACES.md).
- Workspace links, shared appearance, restricted browser storage, responsive
  widths from 320 to 1440px, keyboard access and cached Activity restoration are
  covered. Paused evidence stays fixed when a restored page reconnects.
- Formatting passed. Rust tests and Clippy with warnings denied passed for
  default features, no default features, and the headless snapshot-notifications
  variant. The full browser suite passed **44 tests**; one existing native
  hidden-tab scenario remains skipped in headless mode.
- The optimized executable passed the explicitly invoked isolated macOS lifecycle
  test, including upgrade, rollback/recovery, configuration ownership checks,
  credential preservation and uninstall.

The optional 24-hour soaks were not run and remain non-blocking.

## Visual review

The production pages were reviewed with synthetic data in desktop and mobile
viewports. Light and dark Logs retain the original chart and error-review layout;
the shared header and workspace switch match AI Activity. The inactive AI entry
page describes how to open an appropriate collector. Retained screenshots contain
only synthetic records:

- [Original Logs workspace, full page](design/workspaces-logs.png)
- [AI Activity workspace](design/workspaces-ai.png)

Browser geometry and keyboard checks support this review; it is not a complete
screen-reader or accessibility audit.

## Installed collector acceptance

The existing local login service was upgraded to the optimized build. Client
configuration, trace credential and owned exporter configuration were compared
before and after and remained byte-for-byte unchanged. Codex stayed running.
Collector restart reset in-memory history, then bounded history was reimported.

A fresh browser context opened the managed dashboard, switched AI Activity → Logs
→ AI Activity, and verified real aggregate counts, visible activity rows, local
connection state and retained appearance. The desktop explanation was visible,
raw error previews were hidden, and no browser runtime errors were reported.
The tracing receiver reported `receiving` after upgrade. This does not constitute
a new provider-by-provider capture audit or a fresh Desktop plugin install.

[Machine-readable evidence](validation/workspaces-2026-11-19.json) retains only
counts, status and verification results, with no native chat identifiers,
payloads, credentials or private filesystem paths.

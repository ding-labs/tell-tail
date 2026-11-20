# Unified application source validation

November 20, 2026 · macOS arm64 · branch `codex/unified-sources` · implementation
through `d7610c0`, documentation through `01a379e`. This validates the source build
and managed local collector. No beta packaging, release, version bump or fresh
Desktop plugin installation was performed.

## Regression and boundary checks

- Formatting, all-target Clippy with warnings denied, and Rust tests passed with
  default features, no default features, and headless snapshot-notifications.
- The complete browser suite passed **47 tests**. One existing native hidden-tab
  scenario remains skipped in headless mode.
- Source registry tests exercise saved configuration, stable identity, paused-state
  restoration, exclusive ownership, duplicate-file rejection, invalid parsing
  options, special-file rejection, corrupted configuration preservation, and save
  failure without losing an active source.
- HTTP integration checks exercise the original parser and error previews, appended
  records, file rotation, missing-file creation, pause/resume, edit, removal without
  file mutation, and collector restart. Cross-origin/missing-intent mutations,
  devices, named pipes and oversized bodies are rejected. Browser DELETE requests
  without Content-Length work. Existing read-only routes remain read-only.
- Browser interactions cover both workspace entry points; per-file metrics and
  previews; freeze/resume; add/edit/remove confirmation; persisted settings;
  validation without losing input; literal display of HTML-like source names;
  responsive layouts down to 320px; navigation back from AI Activity; and changes
  made from another tab. Polling leaves unchanged keyboard controls in place.
  Removed sources stop showing old metrics and expose an explicit unavailable state.
- The optimized executable passed the explicitly invoked, isolated native macOS
  lifecycle test, including upgrade, recovery, ownership checks and uninstall.

The two 24-hour soaks remain optional and non-blocking; they were not run.

## Visual review

The actual embedded UI was reviewed in both themes, with the existing Tell-Tail
header and workspace switch retained. Application-source controls are available
through one shared dialog; the original Logs layout and AI Activity feed remain.
Add/edit forms reflow on mobile, use native modal focus behavior and expose inline
validation. Screenshots use synthetic fixture files only:

- [Application log management](design/log-sources-management.png)
- [Application sources alongside desktop capture](design/log-sources-activity.png)

This review includes browser layout and keyboard tests, not a complete
screen-reader or accessibility audit.

## Installed collector acceptance

The existing managed collector was upgraded and an isolated synthetic application
file was added through the browser form. Its error preview and subsequent append
appeared in the original metrics dashboard. The source was paused, the managed
collector restarted through its upgrade flow, and the paused source and parsing
settings were restored. Its metrics correctly remained unavailable until resumed.
Resume reimported the fixture according to its configured start position, then
followed another appended record.

The source was removed through the confirmation UI. The test file was verified
unchanged by removal, existing source configuration was preserved, and the test
fixture was cleaned up. AI Activity remained connected. Client configuration,
trace credential and owned exporter bytes matched before and after both upgrades.
Codex stayed running; collector history reset as expected. No private log payloads
or account data were saved as evidence.

[Machine-readable results](validation/log-sources-2026-11-20.json) record the checks
and receiver status. This is managed-source persistence verification, not a new
provider-by-provider tracing audit. See [application log sources](LOG-SOURCES.md)
for the distinction between original log previews and metadata-only AI capture,
restart/read-position behavior, persistence locations and bounds.

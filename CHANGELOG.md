# Changelog

## 0.2.0 (unreleased)

### Added

- `--apple-unified` streams macOS unified logs directly, without a shell pipeline
  or `jq`. `--format apple-unified` parses saved NDJSON captures on macOS/Linux,
  maps Apple's message types, and excludes stream metadata from entry counts.
- `--output web` serves an embedded local metrics dashboard over SSE, with exact
  counters, rolling rates, severity counts, completed-second history, diagnostics,
  freeze/resume, and connection status. `--open` launches the default browser;
  `--web-port` optionally selects a fixed loopback port. EOF retains final stdin
  metrics until Ctrl+C. Web support is default-enabled and can be omitted with
  `--no-default-features`; CLI JSON schema 2 is unchanged.
- Optional `--alert-error-percent` and `--alert-webhook` send a fixed JSON payload
  with a custom message when the rolling error percentage meets a threshold.
  Minimum samples, breach suppression, and cooldown limit noise. HTTP(S) delivery
  uses bounded state, request deadlines, and prompt cancellation; failures are
  best effort and do not stop ingestion. See the README for payload and options.
- Threshold webhooks also send one `error_threshold_recovered` notification when
  an eligible rolling window falls below the threshold after a queued breach.
  Recovery bypasses breach cooldown, uses a separate message, and requires the
  same minimum sample size. Empty windows and shutdown do not signal recovery.
- `--slack` formats breach and recovery notifications for a Slack incoming
  webhook, including the message, window statistics, source, and session. Use it
  with `--alert-webhook` and `--alert-error-percent`; no formatting relay is needed.

### Changed

- JSON reports now use `schema_version: 2`. Consumers that validate the schema
  version must explicitly accept version 2. Existing fields remain and
  `window.resolution_seconds` is added with value 0.1.
- Rolling windows now retain the requested duration plus less than 100 ms of
  older data, using exact integer counts in 100 ms buckets. Previously a window
  included the current partial second and W-1 preceding seconds, causing short
  windows to reset at each whole-second boundary. Use `observed_seconds` for the
  actual covered duration. Cumulative counts and ratio denominators are unchanged.
- Prometheus metric names, labels, and types remain unchanged; its window ratios
  and observed-duration gauge follow the new window coverage.
- Stdin accepts regular files, pipes, stream sockets, and `/dev/null`. Other
  devices and datagram sockets fail immediately instead of risking a blocked read.

### Fixed

- Unpaired Unicode surrogate escapes in ignored JSON fields now count as parse
  failures. Valid escaped pairs and large numbers in ignored fields still work.

- Stdout status flags are preserved. A bounded helper isolates blocked writes;
  metrics-file output does not initialize stdout.
- A complete stdout report/frame has a two-second deadline. Shutdown, terminal
  cleanup, and runtime diagnostic writes have bounded waits. Output errors return
  status 1 even when shutdown was requested by a signal.
- Fullscreen initialization no longer waits for a cursor-position reply.

- Compact dashboards now expose unknown counts and parse failures.

### Validation and operations

- Added classification fixtures, generated properties, isolated sanitizer fuzz
  targets, exact-count/resource soak tooling, and paired throughput benchmarks.
- Added output compatibility checks, terminal freshness observation, dependency
  auditing, clean-package installation, and separate fast/extended CI tiers.
- Added Linux service and troubleshooting examples. Release-duration and paired
  performance gates remain open; see `docs/VALIDATION-0.2.md`.

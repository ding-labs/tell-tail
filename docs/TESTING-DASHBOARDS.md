# Validating coverage, history, freshness, and onboarding

This guide covers the dashboard, parser, and built-in demo features in 0.3.0. It complements the [broader validation commands](TESTING-0.2.md), which
cover packaging, fuzzing, performance, and extended campaigns. Run commands from
the repository root unless specified otherwise. The repository pins Rust 1.88.0;
browser development checks require Node 22+, npm, and Chromium. The installed
Tell-Tail binary itself does not require Node or Python.

## Automated checks

Run the Rust checks in all supported feature configurations. Tests build their
own executable; rebuild the default binary afterward for the external scripts.

```sh
cargo fmt --check
cargo test --locked
cargo test --locked --no-default-features
cargo test --locked --no-default-features --features snapshot-notifications
cargo clippy --all-targets --locked -- -D warnings
cargo clippy --all-targets --locked --no-default-features -- -D warnings
cargo clippy --all-targets --locked --no-default-features --features snapshot-notifications -- -D warnings
cargo build --locked
python3 scripts/check_terminal.py
python3 scripts/check_terminal.py --alerts
python3 scripts/check_output.py
python3 scripts/check_backpressure.py
```

The terminal script uses a pseudo-terminal and requires a supported Unix host.
Its alert check uses a local receiver, not a real Slack or Discord destination.
For browser checks:

```sh
node --test browser-tests/frame.test.mjs
npm --prefix browser-tests ci --ignore-scripts
npm --prefix browser-tests exec -- playwright install chromium
npm --prefix browser-tests test
# In a desktop session, also exercise native tab hiding and showing:
RUN_HEADED_TESTS=1 npm --prefix browser-tests test
# On a Linux host with Xvfb installed, use this instead of the desktop command:
# RUN_HEADED_TESTS=1 xvfb-run -a npm --prefix browser-tests test
```

The pure frame tests need no Playwright installation. The default browser suite
skips the native visibility test; do not report it as passed unless the headed
run executes it. Browser tests use `target/debug/tell-tail` by default. To test a
release binary, build it and set `TELL_TAIL_BIN=target/release/tell-tail` for the
npm invocation. A browser suite against an old binary is not validation of the
current source. Dependencies and Chromium may need network access during setup.

| Behavior | Regression coverage |
| --- | --- |
| Common format fixtures, ambiguous fields, profile boundaries, nested paths, and container fragments | [`log_styles.rs`](../tests/log_styles.rs), [`format guide`](LOG-FORMATS.md) |
| Coverage includes unknowns, distinguishes no observations, expires with the window | [`stats.rs`](../src/stats.rs), [`frame.test.mjs`](../browser-tests/frame.test.mjs) |
| Completed-second alignment, idle zeroes, ring reuse, independence from window length | [`stats.rs`](../src/stats.rs), [`web_frame.rs`](../tests/web_frame.rs) |
| Exact large counters, optional fields, malformed history and impossible ages rejected | [`web_frame.rs`](../tests/web_frame.rs), [`frame.test.mjs`](../browser-tests/frame.test.mjs) |
| Weighted comparison of adjacent 30-second intervals, including empty denominators | [`frame.test.mjs`](../browser-tests/frame.test.mjs) |
| Record ages, freeze/resume, retained final values, exact history tables | [`dashboard.test.mjs`](../browser-tests/dashboard.test.mjs) |
| Stale/disconnected status, burst coalescing, session reset and live history wrap | [`live.test.mjs`](../browser-tests/live.test.mjs) |
| Terminal layout, severity sparklines, compact coverage and activity | [`tui.rs`](../src/tui.rs), [`check_terminal.py`](../scripts/check_terminal.py) |
| Demo phases, mixed-format evidence, intentional numeric/conflicting unknowns, partial reads without an accumulating queue | [`demo.rs`](../src/input/demo.rs) |
| Demo profiles, custom/escaped paths, literal fields, and Docker/CRI combinations across read boundaries | [`demo.rs`](../src/input/demo.rs), [`cli.rs`](../src/cli.rs) |
| Demo stdin independence, profile/envelope/selector CLI wiring, and cancellation with a final report | [`live.rs`](../tests/live.rs) |
| CLI JSON schema remains unchanged | [`web_frame.rs`](../tests/web_frame.rs), existing output fixtures |

For a focused demo regression run:

```sh
cargo test --locked --lib input::demo
cargo test --locked --lib cli::tests::demo
cargo test --locked --test live built_in_demo
```

The unit cases exercise representative phase times without waiting through a
40-second cycle. They feed generated records through the real engine, checking
severity totals, parser evidence, ambiguous-record counters, and zero malformed
records. Selector cases cover all four profiles, escaped JSON paths, custom
keys (including `message`), and both envelopes with 1-byte, 7-byte, and large
reads. Live cases verify that the actual CLI passes flags to the source and
shuts down correctly; they do not substitute for watching a full cycle below.

Test counts are revision-dependent. Preserve actual command output rather than
copying a count from an older milestone document. A pass on one host does not
establish performance or browser compatibility on other platforms.

## Manual first-use and interpretation checks

Build once with `cargo build --locked`, then run:

```sh
./target/debug/tell-tail --demo --window 5 --output web --open
```

Observe at least one 40-second cycle and continue past 60 seconds for the browser
comparison. Use the [phase table](DEMO.md#what-happens-during-each-cycle) for expected timing.

1. **Startup:** the source identifies synthetic input; coverage becomes 100% as
   recognized records arrive. Before a complete second, history has no points.
   Expand parsing coverage: text, JSON, logfmt, and syslog session counts grow.
2. **Warning and error phases:** warning and error histories change while volume
   stays approximately steady. Expand the exact tables and check that severity
   counts never exceed total records for the matching interval.
   After the window fills with this phase, coverage is about 80%; numeric and
   conflicting-field hints appear. Unknowns increase but parse failures remain
   zero. Recent errors include text, JSON, logfmt, color, and syslog examples.
3. **Quiet phase:** last-record and last-error ages increase, and the live
   connection remains live. After the five-second window expires, coverage and
   ratios are unavailable, not zero. Session totals and previews remain.
4. **Recovery:** new info records reset last-record age; last-error age continues
   increasing. Recovery is an observed change in the synthetic stream, not a
   claim about an application's health.
   Rolling coverage returns to 100%; session parsing counts and hints persist.
5. **Freeze:** focus Freeze display and press Space. Counts, ages, charts, and
   previews hold while connection status stays live. Resume jumps to current
   values. There is no global browser space shortcut.
6. **Comparison:** after 60 completed seconds, the two adjacent 30-second
   intervals appear. Their percentages use summed error/total counts and can
   differ from the five-second rolling card.
7. **Presentation:** inspect a narrow browser viewport and light/dark appearance.
   Check keyboard focus, exact-value disclosures, and readable labels without
   relying only on color. Stop the server with Ctrl+C.

Repeat in the terminal with `./target/debug/tell-tail --demo --window 5`. Use
space, `?`, Escape, arrows, `f`, and `q`. Resize through 100 × 26, 60 × 20,
60 × 18, and a compact size. Recent previews need 60 × 20; the full metrics layout
needs 60 × 18. Small views may clip longer activity/help text. The terminal
must restore after quitting, signals, and input completion.

Restart the browser demo with each flag combination below. Keep `--demo
--window 5 --output web --open` and append the selected flags. Allow at least
24 seconds per run to observe warnings, errors, and raw error previews.

| Additional flags | Expected evidence and previews |
| --- | --- |
| `--profile pino` | Profile counts; error preview contains numeric `"level":50` |
| `--profile python` | Profile counts; error preview contains numeric `"levelno":40` |
| `--profile otel` | Profile counts; error preview contains numeric `"severityNumber":17` |
| `--profile serilog` | Profile counts; error preview contains `"@l":"Error"` |
| `--level-field priority` | JSON counts; the literal `priority` field carries the severity |
| `--level-path /log/level` | JSON counts; severity is inside a `log` object |
| `--profile python --level-path /event/priority` | Profile counts; nested `priority` contains numeric `40` for errors |
| `--envelope docker` | Mixed rule counts and intentional unknowns remain; previews retain the outer JSON wrapper |
| `--envelope cri --profile pino` | Profile counts; previews retain the timestamp, `stdout F`, and inner numeric JSON |

Profile/field/path runs should have no unknowns or parse failures at the default
record-size limit, with 100% coverage in nonempty windows. Envelope-only runs
preserve the default demo's ambiguity. These flags generate matching synthetic
input; do not describe the result as reclassifying the original mixed stream.

Use real parser cases as well: the synthetic demo mixes formats and intentional
ambiguity, but does not include malformed records. This finite example has four
records, one recognized severity, one malformed record, and two otherwise
unrecognized records:

```sh
printf '%s\n' 'INFO ready' '{broken' '{"level":30}' 'request complete' |
  ./target/debug/tell-tail --output web --open
```

Expect 25% rolling coverage, three unknowns, one parse failure, and zero errors.
The web server retains this final frame and its ages until Ctrl+C. Repeat with
`--output json` and check that no `activity`, `severity_history`, or
`recent_errors` fields appear in CLI schema 2. An empty input instead has no
coverage percentage. For followed-file checks, start with an existing file,
confirm EOF-following guidance, append a partial line, and then finish it with a
newline: only the completed record resets activity age. Use `--from start` to
confirm the onboarding guidance includes preexisting lines when requested.

## Freshness, performance, and evidence

For end-to-end terminal freshness, install the pinned development dependency in
a virtual environment and run the existing observation harness:

```sh
python3 -m venv work/validation-venv
work/validation-venv/bin/pip install -r scripts/requirements-validation.txt
work/validation-venv/bin/python scripts/check_freshness.py
```

This measures producer-write-to-visible-terminal acknowledgment under controlled
low load. It is distinct from the **activity age** shown in a snapshot, and from
browser transport freshness. The built-in demo is paced, synthetic input and is
not a throughput benchmark. Use the [paired benchmark procedure](TESTING-0.2.md#performance-comparisons)
when a change could affect ingestion or display overhead; run it separately from
compilation, browser tests, and other heavy jobs.

Keep the source commit/dirty state, binary identity, tool versions, commands,
stdout/stderr, and any screenshots or benchmark JSON with a validation result.
Record skipped checks and platform limits explicitly. Historical results apply
to their named revision, not automatically to current main.

The stdin/NDJSON and followed-file/metrics **24-hour soaks are optional,
non-blocking background validation**. They must not delay release or publication,
require a waiver, or become required CI statuses. Retain and review their results
and report actionable failures separately. This documentation does not add a
new long-duration gate.

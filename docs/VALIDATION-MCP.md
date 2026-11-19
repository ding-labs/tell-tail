# MCP watcher implementation validation

For hosted user documentation, visit the
[MCP guide on Ding docs](https://docs.ding.ing/tell-tail/mcp/).
The [local MCP guide](MCP.md) describes this checkout's implementation;
the [publication checklist](RELEASING.md#hosted-documentation) covers updating the site.

Tell-Tail 0.4.0 now includes this feature in normal installations. The
[0.4 release validation](VALIDATION-0.4-RELEASE.md) records exact-source gates and
public-package checks. The historical implementation measurements below were
taken before release on November 12, 2026, on macOS / Apple M3 arm64, Rust 1.88.0;
they retain their original source scope rather than approving later commits.
Machine-readable results are in
[`validation/mcp-watcher-2026-11-12.json`](validation/mcp-watcher-2026-11-12.json).

## Implemented and checked

The vertical slice in [the plan](PLAN-MCP.md) is implemented in separate focused
commits: model, bounded discovery, collection, Codex adapter, Claude adapter,
scoped state and deduplication, CLI, protected web API, dashboard, and synthetic
demo. Follow-up commits tightened execution isolation, idle publication, fair
source/history selection, main help, clipboard behavior, and package acceptance.

The existing framing and platform identity helpers are reused. MCP collection
has its own scheduler rather than changing the general log follower. Safe fixed
metadata replaces raw evidence excerpts; unrecognized timezone forms remain
undated. Protocol request IDs are not retained or paired. Issue groups retain
individual execution scopes rather than merging them into a global server state.
The optional Claude application-log adapter and plugin integration remain later
work, as described in the plan.

| Check | Result |
| --- | --- |
| Default Rust test suite | 173 passed |
| No-default-features Rust suite | 143 passed |
| Snapshot-notifications-only Rust suite | 149 passed |
| Clippy, all targets, warnings denied | Passed in all three feature configurations |
| Formatting | Passed |
| Existing output isolation, terminal, terminal with alerts, and backpressure scripts | Passed |
| Chromium suite against release binary, including headed visibility test | 33 passed, none skipped |
| Cargo publish dry run (no upload) | Passed from a clean checkout at `af9e9cd` |
| Offline installation from extracted crate | Passed in a clean temporary install root |
| Installed MCP `--open`, assets, safe overview, failed-opener fallback, SIGTERM | Passed |
| Synthetic MCP lifecycle | Failures, recovery, normal closes, unexpected closes, and mirror pairs passed |

The full-suite, package and performance baseline was recorded before the final
unknown-server recovery guard (`bf17df0`). That guard also passed seven scoped-state
unit tests, eight MCP CLI/web integration tests, the release-binary MCP browser
check, and all-target default Clippy with warnings denied. The release binary
was rebuilt afterward. Unidentified/sanitized server names cannot confirm recovery.

The browser MCP check exercises historical labels, issue inspection, clipboard
redaction, Escape dismissal, pause while capture continues, filtered history,
recovery, idle source ages, disconnect labeling, no external requests, and
1440/390/320-pixel layouts. Unit/integration cases cover source selection,
bootstrap boundaries, independent partial records, rename/truncate, new files,
malformed/escaped input, scope isolation, late evidence, bounded retention,
query validation, access restrictions, stream capacity, and idle coalescing.
The 675-pair mirror fixture produces 675 observations; the 43-lifecycle fixture
keeps 42 normal shutdown sequences out of the failure groups.

A verification run accidentally overlapped feature builds that share the same
`target/debug/tell-tail` path. This made a no-web argument test launch the web
binary and wait. It was stopped, then the affected matrix was rerun sequentially
and passed. Feature-matrix commands should remain sequential.

## Measured release performance

The benchmark creates 32 synthetic files containing exactly 16 MiB, opens a real
Chromium page, appends 30 diagnostic events, creates another file, measures a
10-second idle interval, and terminates the watcher. It measures completed-file
writes to rendered timeline rows, not host-side log buffering.

| Measurement | Observed | Target |
| --- | --- | --- |
| First usable browser view | 247 ms | < 2,000 ms |
| Append-to-browser p95 | 300 ms | < 500 ms |
| New file to browser | 149 ms | < 2,000 ms |
| Idle CPU, one-core equivalent | 0.30% | < 1% |
| Watcher RSS | 5.4 MiB | < 128 MiB |
| Graceful shutdown | 128 ms | < 1,000 ms |

These are reference-host measurements, not guarantees for every filesystem,
browser, or future client log format. CPU/RSS use `ps` for the watcher process;
browser memory is not included. Idle CPU has the resolution limits of a
10-second sample. Startup excludes launching Chromium itself but includes starting
the watcher, loading history, navigation and rendering. The benchmark asserts
these targets only when invoked explicitly; it is not a timing-sensitive CI test.

## Passive check on the actual machine

A 60-second read-only run of the release binary automatically selected 60 Codex
and four Claude sources. It imported 2,199 MCP observations and matched 675
aggregate/server mirrors. It exposed 218 unrecognized MCP records and 199 event
evictions. The 64-source cap was reached, and approximately 54.7 MiB of historical
bytes were omitted under the bootstrap budget; these limitations were visible.
No source-open failures occurred. The recording stores counts only, not server
names, raw records, identifiers or payloads. No client or server configuration
was changed.

Three additional non-MCP/unparsed lines arrived, but no new recognized MCP event
arrived during the watch. **Naturally occurring live MCP failure detection remains
unverified on this machine.** Controlled file replay establishes live collector,
parser, state, API and browser behavior without inducing real server failures.

## Reproduce

```sh
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --no-default-features
cargo clippy --all-targets --locked --no-default-features -- -D warnings
cargo test --locked --no-default-features --features snapshot-notifications
cargo clippy --all-targets --locked --no-default-features --features snapshot-notifications -- -D warnings
cargo build --release --locked
TELL_TAIL_BIN=target/release/tell-tail RUN_HEADED_TESTS=1 npm --prefix browser-tests test
TELL_TAIL_BIN=target/release/tell-tail node browser-tests/mcp-benchmark.mjs
python3 scripts/check_package.py
```

Browser checks require the pinned development dependencies and Chromium; see
[dashboard validation](TESTING-DASHBOARDS.md). On a headless host omit
`RUN_HEADED_TESTS=1` (and report the visibility skip), or use Xvfb. The installed
binary needs neither Node nor Python. Package validation uses a fake browser
opener for a safe, deterministic failure check; it does not publish a crate.

## Remaining external acceptance

The five-developer usefulness/onboarding pilot has not been conducted. Record
install-to-first-view, ability to identify a seeded failure within a minute,
false issue reports, and missing evidence before claiming developer validation.
Automatic client discovery is verified only on macOS; explicit fixtures support
Linux tests but do not establish Linux client discovery or Windows support.
A fresh natural MCP event/failure and additional client-version fixtures should
be recorded when available. No claim of complete MCP traffic capture is made.

The 24-hour stdin/NDJSON and followed-file/metrics soaks were not run for this
increment. They remain optional, non-blocking background validation and never
become CI, publish or deployment gates.

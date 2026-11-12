# Tell-Tail 0.4.0 release validation

Frozen source: `09df02f7c233b819b94c08c6229b2ea0934ec206`. This candidate
adds installable passive MCP diagnostics and preserves the 0.3 demo and parser
features. Application and packaging sources are frozen; later evidence commits
do not change the candidate.

The feature is merged in [PR #26](https://github.com/ding-labs/tell-tail/pull/26).
All nine required release gates have passed reviewed evidence for this exact
source. No 0.3 evidence approves this candidate. The machine-readable
[`packaging/validation.json`](../packaging/validation.json) now approves
publication. Publication completed on November 12, 2026 across GitHub Releases,
crates.io and Homebrew. The [publication receipt](validation/0.4.0-release/publication.json)
records the verified source and channel identities.

| Campaign | Run |
| --- | --- |
| Correctness, compatibility, packages, audit | [43307355126](https://github.com/ding-labs/tell-tail/actions/runs/43307355126) |
| Native archives and installation | [43307362571](https://github.com/ding-labs/tell-tail/actions/runs/43307362571) |
| Extended dashboard, resources, fuzzing, performance | [43307353008](https://github.com/ding-labs/tell-tail/actions/runs/43307353008) |
| Controlled Linux paired performance | [43307357400](https://github.com/ding-labs/tell-tail/actions/runs/43307357400) |

## Reviewed short checks and installation

The [CI summary](validation/0.4.0-release/ci-summary.json) records passing
Mac/Linux checks, fresh source-package installations, archive identities, and
report hashes. Six short reports independently match expected records, bytes,
severity, and diagnostics. Both platforms produced the same 202-file source
package checksum. Default, no-default-features, and notifications-only Rust
checks pass. All 33 Chromium tests passed locally against the frozen release
binary, including the headed test, with none skipped.

All three native archives passed runtime, embedded-web, installed-MCP, opener
failure, and temporary Homebrew checks. Linux archives also passed the configured
Debian and Alpine userspace checks. Downloaded archive sizes and SHA-256 hashes
match the exact-source manifest. The extracted Mac archive passed
[eight parser examples and demo phases](validation/0.4.0-release/launch-smoke.json)
and the ordinary [`tell-tail mcp --open` command](validation/0.4.0-release/mcp-launch-smoke.json)
with automatic discovery and a captured browser-opener invocation. The latter
records no private source paths, labels, or log contents. A browser-quarantine
installation and Developer ID signing are not claimed.

The audit reports zero vulnerability-class findings and two informational
unsoundness advisories. The [candidate-specific dependency review](validation/0.4.0-release/dependency-review.md)
checks the affected APIs across all three target dependency graphs, including
the new MCP code. These unused-API findings are disclosed, not ignored.

## MCP reference-host measurements

The [MCP report](validation/0.4.0-release/mcp-performance.json) identifies the
clean frozen source/harness and executed binary. On Apple M3/macOS, it exercises
32 synthetic files and 16 MiB history, 30 appended events, new-file discovery,
a ten-second idle interval, and shutdown through a real Chromium page.

| Measurement | Result |
| --- | --- |
| First usable view | 399 ms |
| Append-to-render p95 | 231 ms |
| New file to render | 1,148 ms |
| Watcher idle CPU | 0.30% of one core |
| Watcher RSS | 5.27 MiB |
| Graceful shutdown | 127 ms |

All benchmark targets passed. Browser memory is excluded, and these reference
measurements are not guarantees for every filesystem or client log format.
The [historical MCP implementation record](VALIDATION-MCP.md) retains its own
source scope and external-validation limits. Naturally occurring live failure
detection and independent first-time user validation are not established by
this synthetic benchmark or by the installed-command startup check.

## Maximum terminal configuration

The [local Mac report](validation/0.4.0-release/maximum-tui-macos.json) ran for
60 seconds with window 3600 and a 16 MiB record limit. All 6,000 records and
1,007,974,560 bytes matched. Combined sampled monitor/helper RSS peaked at
28.86 MiB; shutdown took 211 ms. Sampling is once per second. This result does
not substitute for the separate native Mac/Linux one-hour campaigns.

## Optional background checks

The stdin/NDJSON and followed-file/metrics 24-hour scenarios are not scheduled
for this release. Under [repository policy](../AGENTS.md), they remain optional,
non-blocking background validation. No waiver or completed day result is required
for publication, and no day result is claimed.

## Reviewed controlled performance

The [controlled performance summary](validation/0.4.0-release/performance-summary.json)
identifies the clean candidate and harness, binary hash, and reference host
`runnervm7aw5e` (AMD EPYC 9V74). Producer and consumer are pinned to separate
CPUs. All 15 nested accounting/resource reports pass, and the five matched
pairs use identical workloads. Median headless/TUI throughput is 7.37M/7.12M
records/s with 2.75% median paired loss. Relative ranges are 0.76%/0.98%, with
no generator-saturation warning. Cached 200-byte text reaches 15.64M records/s
with 0.19% relative range. These satisfy the existing reference-host thresholds;
they are not universal throughput guarantees.

The additional Mac performance run is inconclusive: headless/TUI relative
ranges are 56.65%/50.00%, and `measurement_qualifies_for_gate=false`. Its small
median difference is not credited as a passing performance result. The controlled
Linux measurement above satisfies the existing performance gate without changing
thresholds or discarding the noisy observation.

## Reviewed fuzzing

All three targets completed 1,801 seconds against the frozen source without
reported crashes or sanitizer findings. Classification executed 34,688,799
cases, framing 46,972,801, and the timed engine 4,093,166. The
[fuzz summary](validation/0.4.0-release/fuzz-summary.json) preserves the exact
source, run, counts, log hashes, and toolchain. These bounded campaigns do not
prove safety for every possible input.

## Reviewed dashboard duration and resources

The [extended summary](validation/0.4.0-release/extended-summary.json) records
clean source/harness identities, report hashes, memory samples, and timings.
Both native PTY runs exceeded 3,600 seconds and processed exactly 3,597,000
records and 38,521,567,920 bytes, with 360 terminal actions covering resize,
help and freeze/resume. All severity and diagnostic counts matched.

| Measurement | macOS 15 ARM64 | Ubuntu 24.04 x86-64 |
| --- | --- | --- |
| Hour-run sampled peak RSS | 7.48 MiB | 9.79 MiB |
| Maximum configuration sampled peak RSS | 37.59 MiB | 27.23 MiB |
| Hour-run observed shutdown | 331 ms | 101 ms |

RSS combines monitor/helper memory, sampled once per second. Both 64/128 MiB
ceilings and the post-warmup ten-minute-median plateau checks pass. Maximum
runs use window 3600 and a 16 MiB record limit for at least 60 seconds; default
runs use window 60 and a 1 MiB record limit. All 49 extended/controlled
accounting reports pass; the largest observed shutdown was 345 ms. The
additional native Linux performance measurement also qualifies.

## Publication and public installation

[Release workflow 43317916073](https://github.com/ding-labs/tell-tail/actions/runs/43317916073)
completed on attempt 2. The first attempt's Mac job never acquired a hosted
runner; GitHub reported macOS ARM64 capacity constraints before any build steps
ran. Rerunning the failed job preserved the source and completed Linux builds.
No public artifacts were replaced during recovery.

The public tag, every archive's `BUILD.json`, and the downloaded crates.io package
identify the frozen source. All public archive hashes match the reviewed build-only
artifacts byte-for-byte. The registry checksum matches both pre-publication source
packages. The publication workflow freshly installed the registry version and
passed the embedded web and MCP smoke checks.

[Homebrew import 43321789634](https://github.com/ding-labs/homebrew-tap/actions/runs/43321789634)
passed tooling and installation on macOS ARM64 and Linux x86-64/ARM64, then
updated the public formula. The formula exactly matches the public release manifest.
The [downloaded public Mac binary](validation/0.4.0-release/public-mcp-launch-smoke.json)
also passed the ordinary `tell-tail mcp --open` command, synthetic demo, embedded
assets, failed-browser-opener fallback, and graceful shutdown. The ordinary-command
check intercepts the opener and retains no private log content.

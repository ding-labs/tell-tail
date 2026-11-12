# Tell-Tail 0.4.0 release validation

Frozen source: `09df02f7c233b819b94c08c6229b2ea0934ec206`. This candidate
adds installable passive MCP diagnostics and preserves the 0.3 demo and parser
features. Application and packaging sources are frozen; later evidence commits
do not change the candidate.

The feature is merged in [PR #26](https://github.com/ding-labs/tell-tail/pull/26).
Publication remains pending the completed, reviewed exact-source campaign. No
0.3 evidence approves this candidate. The machine-readable approval will be
updated only after all required results have been reviewed.

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

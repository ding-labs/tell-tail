# Error-preview ingestion cost

RC1 source `33f2f9ac959350bcd72332f5a62b53480361c933` failed the dashboard
performance gate in [controlled Linux validation](https://github.com/zuchka/tell-tail/actions/runs/40055923758):
51.83% median throughput loss, with low timing variation and no producer
saturation. A separate Linux run reproduced it. The interactive path decoded,
sanitized and allocated a string plus `Arc` for every error/fatal record,
including millions of entries replaced before a dashboard refresh.

Commit `327d5bcb395ef3aece1786cfbc2eabf0f7fdaf38` captures at most 512 raw bytes
in each of 20 reusable slots. Snapshot publication converts only the retained
entries into immutable, sanitized previews. The same newest-first order,
record/time/level metadata, UTF-8 byte limit, control-character replacement,
truncation and frozen-snapshot ownership apply. Machine outputs still retain no
previews. The change bounds retained raw storage to 10 KiB plus metadata and
moves string allocation/decoding out of per-record ingestion.

## Local comparison

Both binaries ran sequentially on the same Apple M3/macOS 27 host, using the
unchanged mixed-input benchmark with five seeded-order headless/TUI pairs and
five-second calibration. Each cohort calibrated its own fixed count; within
each pair both modes consumed identical records, bytes and fixtures. The RC1
binary came from its verified native release archive; the proposed binary was
built locally with the pinned Rust 1.88.0 toolchain. Both source checkouts and
the common harness were clean.

| Observation | RC1 | Proposed fix |
| --- | ---: | ---: |
| Fixed records per measured mode | 61,798,400 | 63,552,000 |
| Median headless records/s | 12.77M | 12.85M |
| Median TUI records/s | 5.56M | 12.46M |
| Median paired dashboard loss | 57.32% | 3.02% |
| Headless throughput range / median | 2.78% | 0.68% |
| TUI throughput range / median | 3.66% | 3.57% |
| Largest producer CPU fraction | 14.57% | 14.89% |

Both measurements meet the existing noise/producer criteria; only the proposed
fix is below the unchanged 10% loss limit. All exact-count checks passed. The
[retained summary](validation/preview-performance-local.json) includes binary,
source and report hashes plus each pair's timings. Full local reports/logs are
in ignored `work/performance/` in the performance-fix worktree. This is local
diagnostic evidence, not validation of a new frozen release candidate.

The full Rust test suite, clippy with warnings denied, formatting and a release
build pass locally. New regressions cover overwritten-slot ownership, shorter
replacement records, invalid-UTF-8 expansion and split codepoints, alongside
existing chunking, terminal/web rendering and preview-safety coverage.
## Controlled Linux confirmation

[Run 40059139178](https://github.com/zuchka/tell-tail/actions/runs/40059139178)
confirms the fix on clean source `327d5bcb395ef3aece1786cfbc2eabf0f7fdaf38`,
using the unchanged clean RC1 harness. The report identifies reference host
`runnervm8k3b6`, AMD EPYC 7763, with producer CPU 0 and consumer/helper CPU 3,
matching the RC1 reference-host identifiers.

Five one-minute-calibrated pairs passed exact counters and byte accounting.
Median headless/TUI rates were 6.66M/6.41M records/s, with **3.76%** median paired
dashboard loss (individual pairs 2.23–5.01%). Headless/TUI ranges of 1.76%/1.65%
and maximum producer CPU use of 35.58% qualify the measurement. Cached text
reached a 20.63M records/s median, above the 1M reference target. The unchanged
performance limits pass for the proposed code. The [retained summary](validation/preview-performance-linux.json)
records complete identities, fixture/report hashes and every pair's timings;
the full report/log is retained under ignored `work/performance/controlled-linux`.

Native tests and archive/Homebrew checks pass on macOS ARM64 and both Linux
architectures. All 21 web dashboard checks also pass after the test launcher
allowed 15 seconds for cold startup, required a complete port record and retained
bounded launch diagnostics. The previous five-second timeout also affected the
unchanged release-record branch; product responsiveness thresholds are unchanged. RC1's tag, Railway binary and release
gates remain unchanged; the proposed code does not certify the frozen candidate.
Publication stays on hold.

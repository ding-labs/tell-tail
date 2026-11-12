# Tell-Tail 0.3.0 release validation

Candidate: `05b3376e2ba1f809bf0a789faa477f3f17e1bcf2`. Version 0.3.0 includes
the merged demo, expanded parser coverage, dashboard interpretation features,
and recovery/rotation/shutdown fixes. Application changes are frozen at this
commit; subsequent validation records and plans do not change the candidate.

All nine required release gates have passed reviewed evidence for this exact
source. No gate is credited from the 0.2.0 campaign. The machine-readable
[`packaging/validation.json`](../packaging/validation.json) approves publication
of this candidate; publication itself is pending. The optional day scenarios
remain outside the release gates.

| Evidence | Run |
| --- | --- |
| Exact-source checks | [43286011546](https://github.com/ding-labs/tell-tail/actions/runs/43286011546) |
| Exact-source native archives | [43286300595](https://github.com/ding-labs/tell-tail/actions/runs/43286300595) |
| Extended dashboard, resources, fuzzing, paired performance | [43286063233](https://github.com/ding-labs/tell-tail/actions/runs/43286063233) |
| Controlled Linux paired performance | [43286068503](https://github.com/ding-labs/tell-tail/actions/runs/43286068503) |

## Maximum-configuration terminal check

The [local macOS report](validation/0.3.0-release/maximum-tui-macos.json)
identifies the clean candidate and harness, Rust toolchain, binary hash, and
expected/actual accounting. The terminal run completed 60 seconds with window
3600 and a 16 MiB record limit. All 6,000 records and 1,007,974,560 bytes matched;
peak combined sampled monitor/helper RSS was 28.66 MiB with one-second sampling.
Shutdown took 153 ms. This is a maximum-configuration observation, not an
hour-long dashboard result or a replacement for native Linux validation.

## Optional background checks

The stdin/NDJSON and followed-file/metrics 24-hour scenarios are not scheduled
for this release. They remain optional, non-blocking background validation under
[repository policy](../AGENTS.md). They do not delay publication or require a
waiver, and no full-day result is claimed.

## Reviewed checks and distribution

The [CI summary](validation/0.3.0-release/ci-summary.json) records passing
Mac/Linux checks, fresh source-package installations, archive identities, and
report hashes. Six short exact-count reports independently match expected
records, bytes, severity, and diagnostics. Visible-count p95 is 295 ms on macOS
and 217 ms on Linux, below the 500 ms limit; observed quit is 281/212 ms.
Both source-package checks produce the same 168-file crate checksum.

All three native archives passed build, runtime, embedded-web, and temporary
Homebrew tests; Linux archives also passed the configured Debian/Alpine checks.
The downloaded archive sizes, hashes, manifest, and checksum document were
independently verified. The extracted Mac archive passed another CLI/web smoke
and [eight parser examples plus demo phases](validation/0.3.0-release/launch-smoke.json).
Direct browser/quarantine installation and Developer ID signing are not claimed.

The audit has zero vulnerability-class findings and two informational
unsoundness warnings in unused APIs. The
[scoped dependency review](validation/0.3.0-release/dependency-review.md) records
API use, source inspection, limits, and the maintenance follow-up. No advisory
is hidden or reported as absent.

## Reviewed controlled performance

The [controlled report summary](validation/0.3.0-release/performance-summary.json)
identifies the candidate, clean harness, binary, and reference host
`runnervm7aw5e` (AMD EPYC 9V74). Producer and consumer use separate CPUs. Fifteen
nested warmup, pair, and idle/resource reports independently pass exact-count
and resource checks. All five pairs use identical workloads.

Median headless/TUI throughput is 7.37M/7.19M records/s, with 2.67% median paired
loss. Streaming relative ranges are 0.70%/0.34%, with no generator saturation
warning. Cached 200-byte text reaches 15.66M records/s with 0.49% relative range.
These satisfy the existing thresholds on the named host; they are not universal
production throughput guarantees.

## Reviewed fuzzing

All three targets completed 1,801 seconds against the frozen source without
reported crashes or sanitizer findings. Classification executed 45,069,300
cases, framing 53,354,969, and the timed engine 3,375,081. The
[fuzz summary](validation/0.3.0-release/fuzz-summary.json) preserves counts, log
hashes, and the toolchain. These are completed bounded campaigns, not a claim
that every possible input has been proven safe.

## Reviewed dashboard duration and resources

The [extended summary](validation/0.3.0-release/extended-summary.json) records
clean source/harness identities, report hashes, memory samples, and timings.
Both native PTY runs exceeded 3,600 seconds, processed exactly 3,597,000 records
and 38,521,567,920 input bytes, and exercised 360 terminal actions for resize,
help, and freeze/resume. Their severity and diagnostic counts matched the
independent expected values.

| Measurement | macOS 15 ARM64 | Ubuntu 24.04 x86-64 |
| --- | --- | --- |
| Hour-run sampled peak RSS | 7.81 MiB | 9.77 MiB |
| Post-warmup ten-minute RSS medians | 6.55 MiB, stable | 9.75–9.77 MiB |
| Maximum configuration sampled peak RSS | 37.39 MiB | 26.84 MiB |
| Hour-run observed shutdown | 325 ms | 159 ms |

RSS is combined monitor/helper memory sampled once per second, not an
instantaneous maximum. Default runs use window 60 and a 1 MiB record limit;
maximum runs use window 3600 and a 16 MiB record limit for at least 60 seconds.
Both the 64/128 MiB ceilings and the post-warmup plateau criterion pass. The
separate fixed-geometry Mac terminal maximum check is described above.
All 49 reviewed extended/controlled accounting reports pass, with no shutdown
observation exceeding 325 ms.

The additional native Linux performance measurement also qualifies (2.07%
median paired TUI loss). The additional Mac performance measurement is
**inconclusive**: headless/TUI relative ranges are 32.80%/42.37%, and the report
correctly sets `measurement_qualifies_for_gate=false`. Its small median paired
loss is not credited as a passing performance result. The named controlled
Linux reference above satisfies the existing performance gate without changing
thresholds or discarding the noisy Mac observation.

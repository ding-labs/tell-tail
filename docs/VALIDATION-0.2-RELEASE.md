# Tell-Tail 0.2.0 publication validation

All nine required release gates pass for source
`02a6476f9921ba45c59a6ce98bd68b317bac79b6`, the exact Git commit already published
as crates.io 0.2.0. Tooling and evidence commits do not change that candidate.
No version or candidate tag is moved. The application source, web assets,
manifest and lockfile remain unchanged from the published source.

The two 24-hour Linux scenarios are optional, non-blocking background checks
under the standing [repository policy](../AGENTS.md). They were not completed
for this release. No release waiver is required or claimed, and those scenarios
are not marked passed. The standalone `background-soaks.yml` workflow has its
own concurrency group and no dependency in publication or short validation.

## Correctness, compatibility, responsiveness and distribution

[Exact-source checks 40619394057](https://github.com/ding-labs/tell-tail/actions/runs/40619394057)
passed on Linux and macOS. The [CI summary](validation/0.2.0-release/ci-summary.json)
records source-package installation, embedded dashboard assets and web-server
shutdown plus hashes of downloaded platform reports. Visible-count p95 was
215/399 ms on Linux/macOS; quit was 212/383 ms, within the unchanged limits.

[Native build 40619393915](https://github.com/ding-labs/tell-tail/actions/runs/40619393915)
passed macOS ARM64 and both static Linux architectures, including temporary
Homebrew installations and Debian/Alpine archive checks. Its downloaded release
manifest and every archive checksum were reverified. Package metadata, included
files and changelog were reviewed; crates.io retains its original source/version.
The immutable source archive's historical README/changelog do not establish
current publication status; current installation availability belongs on main
and the GitHub release page.

## Extended results reviewed October 28, 2026

[Extended validation 40789256596](https://github.com/ding-labs/tell-tail/actions/runs/40789256596)
completed successfully. The [review summary](validation/0.2.0-release/extended-summary.json)
retains source/harness/binary identities, report hashes, per-pair measurements,
resource observations and fuzz counts. Independent report review verified exact
expected/actual records, bytes, levels and diagnostics for 49 complete dashboard,
resource, warmup, performance, idle and followed-file reports.

- Each sanitizer fuzz target completed 1,801 seconds without a finding:
  classification 43,391,456 executions, framing 35,122,103, timed engine 3,848,663.
- Both native interactive dashboards exceeded 3,600 seconds, sent 360 terminal
  actions and verified exact final accounting. Peak app/helper sampled RSS was
  7.03 MiB on macOS and 9.13 MiB on Linux. Complete ten-minute medians remained
  stable after warmup; sample interval was one second. Largest observation gaps
  were 229 ms and 33 ms respectively.
- The one-minute maximum-configuration headless runs used window 3600 and a
  16 MiB record limit, peaking at 37.13 MiB on macOS and 26.09 MiB on Linux.
  The separate [macOS terminal run](validation/0.2.0-release/maximum-tui-macos.json)
  passed 60 seconds and 6,000 exact records at 26.70 MiB. All remain below 128 MiB.
- Across reviewed idle and loaded reports, the largest shutdown was 311 ms,
  within the one-second cancellation requirement. Terminal freshness remains
  established by the original exact-source visible-output checks above.

## Performance

[Controlled Linux run 40789260268](https://github.com/ding-labs/tell-tail/actions/runs/40789260268)
built the frozen source with clean harness
`09172e83d3535c4355c8b71c3712d2cd3b580b80`. Reference host `runnervm48cm3`
reported AMD EPYC 9V74, producer CPU 0 and consumer/helper CPU 3. Five matched
pairs verified exact accounting. Median headless/TUI rates were 7.25M/7.05M
records/s, with **2.68%** median dashboard loss. Relative throughput ranges were
1.15%/0.96%; producer utilization stayed below the unchanged 75% limit.
Cached 200-byte text reached 20.43M records/s with 9.46% range, satisfying the
named-host absolute and noise thresholds.

The independent native extended measurements also qualified: Linux reported
2.83% paired loss at 10.93M/10.54M records/s; macOS reported 0.56% loss at
9.94M/10.01M records/s. All have five seeded-order pairs, one-minute calibration,
unchanged source identities, no producer warning and streaming variation below
10%. Some ancillary cached fixture timings were noisy; they are preserved and
are not used for the qualifying text reference. Absolute rate comparisons across
these different hosts are not general production throughput guarantees.

## Preserved optional campaign and publication

The temporary Railway day campaign was stopped at the user's direction. Its
[startup identity](validation/0.2.0-release/linux-start.json) and
[cancellation record](validation/0.2.0-release/linux-cancelled.json) are retained;
its result volume remains. No partial duration earns full-day credit and the
campaign will not be restarted for this release.

All raw CI/extended/performance reports, full fuzz logs, partial day evidence,
review script and SHA256SUMS are retained under ignored `work/release-0.2.0/`.
Original native archives are under `work/distribution/0.2.0-02a6476/`. The previous
candidate record remains in [previous-validation.json](validation/0.2.0-release/previous-validation.json).

After this evidence PR's CI passes, merge it preserving commits and dispatch
`release.yml` on main with `publish=true` and the frozen full source SHA. The
user authorized publication in this chat. Retain main-only and protected release
environment approval. The workflow must reuse the existing exact-source crate,
publish GitHub archives and checksums, and finish the Homebrew importer before
publication is reported complete. Update current installation docs afterward.

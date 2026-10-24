> Current candidate: [0.2.0 RC1 validation](VALIDATION-0.2-RC1.md).
> This document preserves historical candidate results and interrupted attempts.

# Tell-Tail 0.2 implementation validation

October 14, 2026. Implementation steps 1–12 are complete. Release validation
(step 13) is incomplete. The package is now 0.2.0 for a candidate validation
campaign; no version tag or release has been published.
This report does not certify a GA or release-candidate build.

The committed [machine-readable summary](validation/0.2-smoke.json) records binary
hashes, fixture hashes, configurations, package results, and measurements. Full
local logs/results are in the ignored `work/` directory. Measurements span the
implementation commits and are not a single final-candidate campaign. Reproduce
checks using [TESTING-0.2.md](TESTING-0.2.md).
The original campaign below predates webhook alerting; its follow-up checks are
recorded separately near the end of this report.

## Platforms and correctness

- macOS 27.0, Apple M3, arm64; Rust 1.88.0.
- Linux 6.10.14 LinuxKit, ARM64 container, Debian Bookworm/glibc 2.36; Rust 1.88.0.
  This is runtime evidence for the local Linux VM/container configuration. There
  is no native Linux x86_64 runtime evidence from this implementation session.

Both platforms pass formatting, clippy with warnings denied, and all 37 Rust
unit/integration tests. Coverage includes 35 classification fixtures across six
chunk sizes and three generated properties with 128 cases each. Dedicated
executable checks cover inherited descriptor flags, blocked
and slow consumers, parent death, broken pipes, both signals, terminal recovery,
file generations, and atomic metrics failure. Removing directory write access
causes status 1 while preserving the last complete metrics file, including when
the test runner itself is root.

Promtool 3.3.0 accepts the compatibility fixture and actual executable exposition.
Actionlint 1.7.12 accepts both workflow definitions. This initial campaign predates
the GitHub repository; subsequent remote runs are available in
[GitHub Actions](https://github.com/zuchka/tell-tail/actions). No dedicated release
runner has been provisioned.

## Responsiveness and short resource runs

| Observation | macOS arm64 | Linux ARM64 |
| --- | ---: | ---: |
| Visible-count freshness, p95 of 25 observations | 244 ms | 249 ms |
| `q` shutdown with terminal restoration | 267 ms | 277 ms |
| SIGTERM with terminal restoration | 156 ms | 172 ms |
| SIGINT with terminal restoration | 158 ms | 169 ms |
| EOF with terminal restoration | 155 ms | 169 ms |
| Stalled PTY shutdown with termios restoration | 764 ms | 800 ms |
| Default mixed-input smoke, combined sampled RSS | 6.36 MiB / 3 s | 5.21 MiB / 3 s |
| Window 3600 / record limit 16 MiB, 100 records/s | 19.66 MiB / 15 s | 22.86 MiB / 15 s |

Freshness observes actual rendered screen counts through a terminal emulator with
one input record per visible acknowledgment. It is a controlled low-load result.
Shutdown measurements include the application's output helper; terminal test
supervisors report restoration separately from draining remaining test output.

Resource samples include the application and descendants, at one-second
intervals. They do not establish instantaneous peaks or a long-run plateau. An
earlier three-second maximum-configuration burst smoke sampled 25.11 MiB on
macOS at a higher input rate. Linux's 12-second TUI smoke exercised six control
and resize actions and verified all 12,000 records; controlled file rotation and
missing-path/partial-record handling also passed exact-count checks.

## Performance remains inconclusive

Five warmed cached-file trials on the M3 produced these medians:

| Synthetic fixture | Records scanned | Median records/s | Measured range |
| --- | ---: | ---: | ---: |
| 200-byte text | 2,000,000 | 20.62M | 16.39M–25.62M |
| 500-byte JSON | 2,000,000 | 1.35M | 1.16M–1.81M |
| 16 KiB text | 65,536 | 315K | 289K–335K |
| Mixed severities/malformed/oversized | 2,000,000 | 5.31M | 4.64M–5.58M |

These synthetic cached scans exceed the absolute reference targets, but have
substantial timing variation. They are not general production throughput claims.

The initial paired pipeline campaign flagged both producer saturation and timing
variation. Its apparent negative median dashboard overhead is inconclusive.
Increasing producer batches from 100 to 6,400 records removes much bookkeeping;
short harness checks pass. A longer rerun still varied sharply (the first three
pairs showed approximately 9.76%, 3.32%, and 54.02% dashboard loss) and a later
fixed workload exceeded its duration limit. Do not claim the <10% overhead gate
passes. Retain that failure and rerun on a controlled reference host; investigate
any repeatable application regression. The harness now saves partial results on
failure as well as successful trial details.

## Fuzzing, dependencies, and packaging

Three sanitizer-backed targets passed 30-second macOS smoke runs. The initial
framing smoke exceeded the sanitizer RSS budget with the default ASan quarantine;
its saved input replayed 10,000 times without a finding. Reducing quarantine to
32 MiB yielded clean bounded smokes. Details and execution counts are in the
[fuzz instructions](../fuzz/README.md). No 30-minute release campaign has run.

Cargo-audit 0.22.2 reports zero vulnerabilities and zero informational warnings
against the recorded RustSec database. No advisory is ignored. See the
[dependency review](DEPENDENCIES.md) for versions and declared licenses.

Cargo package build verification and installation from an extracted source
package pass on both platforms. macOS used a clean Git checkout. Linux used a
source copy without Git metadata and the explicitly recorded `--allow-dirty`
option; installation still used a fresh temporary directory. The check caught
and fixed toolchain selection outside the source directory and Python 3.11's
missing tar extraction-filter API. Package contents exclude logs, fuzz corpora,
build products, and Python caches.

## Webhook follow-up

The webhook implementation through `3de572e` passes all 52 Rust tests on macOS
arm64 and the Linux ARM64 container described above, using Rust 1.88.0. New tests
cover inclusive thresholds, fatal/unknown counts, minimum samples, rearming,
cooldown, custom-message escaping, finite EOF, failure suppression, redirect
rejection, bounded queues, parent death, stalled requests, and both signals.
A stalled endpoint does not prevent exact counting of another 200,000 records.

The terminal check with `--alerts` confirms delivery while the display is frozen,
then verifies keys, resize, final counts, and terminal restoration. All four exit
paths complete within one second on both platforms. JSON, text, Prometheus stdout,
and metrics-file delivery are also covered. HTTP receivers in these checks are
local test fixtures; no external notification service is contacted.

Formatting, clippy with warnings denied, actionlint, and locked fuzz-workspace
metadata checks pass. RustSec reports zero advisories and warnings across the
updated 154-dependency lockfile. Source-package build verification passes on
macOS, as do the existing inherited-output, backpressure, stalled-terminal, and
metrics-publication-failure scripts. The earlier throughput/resource measurements
and long-duration release gates have not been rerun for this addition.

## Gates still open

1. Run 30 minutes per fuzz target and resolve every finding.
2. Run separate 24-hour Linux stdin/NDJSON and followed-file/metrics scenarios.
3. Run one-hour dashboard scenarios on macOS and Linux, including interactions;
   evaluate ten-minute memory medians after warmup.
4. Establish a stable paired performance result below 10% loss, plus final
   candidate responsiveness/resource evidence.
5. Execute remote CI and obtain native runtime evidence for each advertised
   architecture, including Linux x86_64 before shipping that archive.
6. Prepare dependency notices and checksummed candidate archives, then validate
   and bump the package/lockfile version together. The project uses the
   [MIT License](../LICENSE), and canonical repository metadata points to
   [zuchka/tell-tail](https://github.com/zuchka/tell-tail).

Release preparation now has fail-closed validation records, native archive and
Homebrew tooling, and a build-only workflow. Local macOS archive extraction and
binary smoke checks pass; a local Homebrew install is blocked by this machine's
outdated Xcode/Command Line Tools. Native CI exercises Homebrew installation
from a temporary local server without publishing. The shared
[Ding Labs tap](https://github.com/ding-labs/homebrew-tap) has a separate import
workflow for a future published stable release. These changes prepare the
distribution gate; they do not complete the remaining 0.2 validation gates.

The workflows and supervised commands were prepared by the implementation
campaign above. The later candidate campaign below starts long-duration checks;
tags, registry publication, and the first public release remain on hold.

## October 14 candidate campaign

The frozen candidate is `ffda8eaebec5e3f08115e622423bd8a30e7d673c` (0.2.0).
Documentation and validation-record commits after it do not replace that source
identity. Release automation can select the candidate explicitly; all gates must
cover it before publication is considered.

Validation is in progress:

| Evidence | Run / state at launch |
| --- | --- |
| Native correctness, compatibility, dependency audit, package/install and short resource checks | [Checks](https://github.com/zuchka/tell-tail/actions/runs/38671783912) |
| Three 30-minute fuzz targets, one-hour dashboards on both native platforms, maximum configuration and five one-minute performance pairs | [Extended validation](https://github.com/zuchka/tell-tail/actions/runs/38671823099) |
| Native macOS ARM64 and Linux x86_64 archives, checksums, notices and temporary Homebrew installation | [Build-only candidate](https://github.com/zuchka/tell-tail/actions/runs/38671826035); `publish=false` |
| Two sequential 24-hour Linux ARM64 container scenarios | Started October 15 at 02:36 UTC; stdin/NDJSON first, then followed file/Prometheus metrics |

The local campaign uses a clean detached clone mounted read-only, Rust 1.88.0,
and a frozen binary with SHA-256
`db11c81409ffbdc8f6ac15b75cca6a6e90a03aa17f91658d4e796a20c5f7ec4c`.
Its Rust Bookworm image digest is
`sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0`.
The runtime is Linux ARM64, separate from the advertised Linux x86_64 archive's
native CI evidence. Each phase has network disabled, two CPUs and a 768 MiB
container limit; Tell-Tail's own combined sampled RSS ceiling remains 64 MiB.
Host sleep prevention follows the container lifetime. Observation gaps over
five seconds invalidate a run rather than counting suspended time as evidence.

The ignored `work/validation-campaigns/<candidate>/` directory retains the image,
compiler and binary identities, configuration, phase journal, minute progress,
preflight and final reports. The three-second preflight passed exact counts
(3,000 records), with 5.86 MiB combined sampled RSS. This is a harness check,
not duration evidence. Final counts, continuity, memory medians, responsiveness
and all completed remote artifacts still require review.

The earlier candidate's extended classification run found a reference-parser
defect: with `raw_value` enabled, `serde_json::Value` interprets the ordinary key
`$serde_json::private::RawValue` specially. Tell-Tail correctly accepted the
valid object. Commit `cfb41b7` replaces the fuzz reference with a fully decoding
visitor that treats keys literally, adds deterministic regressions and retains
a synthetic seed. The saved input passes 10,000 replays; a fresh macOS smoke
completed 1,348,248 executions in 31 seconds. A new 30-minute campaign is required.

The earlier ten-second paired performance runs reported median TUI losses of
2.03% on macOS and 4.89% on Linux, but their throughput ranges exceeded the 10%
noise limit. Both measurements remain **inconclusive**. The one-minute rerun
preserves the existing qualification and overhead limits. The earlier local
day run and remaining extended jobs were explicitly stopped after the harness
fix; their partial durations do not pass any gate. The superseded candidate's
[normal CI](https://github.com/zuchka/tell-tail/actions/runs/38670931081) and
[build-only packaging](https://github.com/zuchka/tell-tail/actions/runs/38671010413)
passed, including native Homebrew installation, with publication skipped.

### Reviewed candidate results

The frozen candidate's full Checks matrix passed on macOS ARM64 and Linux
x86_64, including the corrected fuzz-reference regressions and three 30-second
fuzz smokes. Schema 2 fixtures, CLI/exit statuses, Prometheus validation,
backpressure, terminal restoration and source-package checks all pass.
Both clean source installations use the same 73-file package, SHA-256
`a2441a602402d2679ead965b6368c6dbba7b2f20f6ed1fbbeff594865e22aad4`, without
`--allow-dirty`. RustSec database `edb58f5974b83eb8302332821cb6d389777aa351`
reports zero vulnerabilities and zero informational warnings for 154 locked
dependencies, with no ignored advisory.

The build-only candidate workflow passed extraction, executable smoke and
temporary Homebrew installation on both advertised native targets. Its manifest
records the frozen source commit and `dirty=false`; local re-verification of
the downloaded archive sizes and checksums also passes. The archives include
Tell-Tail's license, collected dependency notices and build provenance.

| Candidate archive | SHA-256 |
| --- | --- |
| `aarch64-apple-darwin` | `6ef7bdc552d18405848e4d33ee598260b4164f027ec1dc608ecf3eafda7c94ea` |
| `x86_64-unknown-linux-gnu` | `37f8fc0677b0202455451cd0a1205e23eb0c2a41e4e840ab78755f4b84f78c71` |

A local release binary built from the frozen source passed 25 terminal-emulator
freshness observations: p95 225 ms, maximum 241 ms, quit 215 ms, at 100 × 26.
Its SHA-256 is
`14e64c93833fd2cbc4a2749f35ba430c90ab4bcea82ed70f0f708cd0c4f9d7c5`.
This remains a controlled low-load observation, not a loaded-throughput claim.

The candidate workflow's publication job was skipped; the release is still
blocked. Completed gates and remaining work are recorded below.

All three extended Linux x86_64 sanitizer-backed fuzz targets completed 1,801
seconds on the frozen candidate without a finding. The downloaded artifacts
record the matching commit and pinned nightly toolchain. Framing completed
43,626,714 cases, classification 34,880,931 and timed engine 4,010,984.
The fuzzing duration gate now passes.

The five one-minute performance pairs remain inconclusive on both hosted
platforms. Linux JSON/TUI throughput ranges were 15.76%/7.67%; macOS ranges were
31.88%/30.13%. Neither producer exceeded the CPU utilization limit. Apparent
median negative TUI losses are not evidence of an overhead pass under these
conditions. Linux cached medians were 23.91M text and 3.40M JSON records/s, with
ranges below 1%; further controlled pipeline measurements are required.

### One-hour dashboards, memory and responsiveness

The completed extended workflow verifies the frozen candidate in native macOS
ARM64 and Linux x86_64 one-hour PTY runs. Each processed exactly 3,597,000 records
through the burst profile and exercised 360 terminal actions, including resize,
freeze/resume and help. Production lasted at least 3,600 seconds. Neither run
had an observation gap approaching the five-second invalidation threshold.

| Observation | macOS ARM64 | Linux x86_64 |
| --- | ---: | ---: |
| Peak combined sampled RSS, default configuration | 6.56 MiB | 8.27 MiB |
| First complete post-warmup ten-minute median | 6.53 MiB | 8.16 MiB |
| Largest subsequent median | 6.56 MiB | 8.27 MiB |
| Resource observations | 3,389 | 3,552 |
| EOF-to-exit after one-hour burst input | 168 ms | 188 ms |
| Maximum configuration, 60-second headless run | 36.84 MiB | 26.76 MiB |

Samples are nominally one second apart; processing and scheduling add drift.
The first ten-minute block is warmup. Subsequent medians remain far below the
allowed growth of 4 MiB or 20%, and default peaks remain below 64 MiB. These
completed runs pass the dashboard and default-memory gates. The day campaign
continues checking the same memory ceiling and plateau; a later failure reopens
the affected gates.

The maximum-configuration headless runs use window 3,600, record limit 16 MiB,
100 records/second and the 128 MiB app/helper ceiling. Additional 60-second TUI
checks at 100 × 26 verify the same configuration and exactly 6,000 records on
local macOS ARM64 (27.72 MiB) and Linux ARM64 Docker (24.40 MiB). The frozen
binary hashes are recorded above; these additional checks use clean validation
harness `5ca6444c6decdf2db997e2ffcd6c224a9276102a` with the source checkout and
harness identities reported separately. Reports are retained locally as
`macos-maximum-tui.json` and `linux-maximum-tui.json` in the campaign results.
This passes the maximum-memory gate; these short maximum-size checks do not
substitute for the default-configuration duration evidence.

The native correctness workflow's controlled signal, stalled-output, terminal
and alert-worker cancellation checks pass their one-second deadlines. Native
terminal-emulator freshness p95 is 325 ms on macOS and 218 ms on Linux, below
500 ms, with draining terminals and one input record per visible acknowledgment.
The local release-binary observation is recorded above. Across the completed
minute-long pipeline, idle and followed-file measurements, the longest observed
shutdown was 329 ms on macOS and 199 ms on Linux. The largest headless snapshot
ages were 308 ms and 118 ms respectively. The controlled responsiveness gate
passes; these observations do not establish terminal freshness under arbitrary
production traffic.

At this checkpoint eight gates passed, with the two 24-hour Linux scenarios and
qualified performance measurements still open. A separate
[controlled Linux measurement](https://github.com/zuchka/tell-tail/actions/runs/38677628380)
builds the same frozen candidate with the newer harness, pins producer and
consumer to separate available logical CPUs and records placement and the CPU
model. The original noisy measurements remain retained and inconclusive.
CPU affinity does not reserve a shared runner or guarantee a qualifying result.

### Controlled Linux reference result

The completed controlled performance run qualifies for the frozen candidate,
using clean harness `5ca6444c6decdf2db997e2ffcd6c224a9276102a`. Its Linux x86_64
binary SHA-256 matches the original candidate's native measurements:
`f06d6ff7dfce699c5aa457f7b83674b1a1edb8062e0edb6709078875bea765e6`.
The named reference host is `runnervmmprz5`, AMD EPYC 9V74, with four available
logical CPUs. Producer CPU 0 and consumer/helper CPU 3 are recorded in the
[retained summary](validation/0.2-performance.json), alongside the full report's
hash, fixture hashes, each pair and both source identities. The GitHub artifact
retains the complete report.

Five seeded-order pairs each process exactly 587,904,000 mixed-profile records
per mode after separate one-minute warmups. Median headless/TUI throughput is
9.81M/9.74M records/s. Median paired dashboard loss is **0.74%**, with individual
losses from -0.25% to 1.22%. Headless and TUI throughput ranges are 1.21% and
0.72%, both below the unchanged 10% noise limit. The largest measured producer
CPU fraction in these pairs is 40.81%, below 75%; no producer-limit warning is
set. All exact-count checks pass.

The cached 200-byte text median is 23.28M records/s, with 1.28% variation. This
satisfies the plan's absolute target of at least 1M text records/s on the named
reference host. The supplementary 500-byte JSON median is 4.02M records/s, but
its 10.15% range is noisy and is **not** used as qualifying absolute-rate
evidence. Earlier noisy pipeline measurements remain inconclusive; this result
does not establish throughput on other machines or production log mixes.

The performance gate now passes. Nine of ten gates are complete; only the two
sequential 24-hour Linux scenarios remain open. The subsequent campaign status
is recorded below. Publication remains blocked until that duration evidence has
completed and been reviewed, and the user's release hold remains in force.

### Docker interruption and clean retry, October 15

The first day-run attempt was interrupted before completion. Its last minute
checkpoint covered 15,903 seconds (4 hours 25 minutes), with 15,903,000 producer
records and 5.76 MiB peak sampled app/helper RSS. These are partial observations,
not verified final counts or passing duration evidence. No final soak report
was written. Docker restored the container as exited with code 255 and
`OOMKilled=false`; the host's `docker wait` connection ended with `unexpected
EOF`. Docker backend logs show its Linux VM starting again at 07:08:59 UTC.
The Mac did not reboot. The reason for Docker's restart is not established.

The original configuration, progress, journal and Docker state are preserved,
along with a separate interruption record. The stale running journal was marked
interrupted while retaining its original contents. None of that attempt's time
contributes to the required two 24-hour runs.

Attempt 2 started at **07:48:01 UTC on October 15**, using the same clean frozen
source, binary hash, image, scenarios and resource bounds. Its container is
`tell-tail-0-2-days-ffda8eaebec5-retry-02`; reports are separate under
`work/validation-campaigns/<candidate>/attempts/02/results/`. A fresh three-second
preflight passed before starting stdin/NDJSON, followed by a separate full day
of file/Prometheus validation. If uninterrupted, both phases finish around
07:48 UTC October 17 (00:48 Pacific).

The host now wraps `scripts/watch_docker_soak.py` in `caffeinate` rather than
wrapping `docker wait`. The watcher retries API disconnects, preserves original
journals and marks an unexpected container exit interrupted. It does not restart
containers, certify results or modify Docker settings. Its source at launch is
`311f9bb`; regression tests cover interrupted exits, misleading zero exits,
completed/failed journal preservation and disconnect recovery. An actual Docker
exit-code-7 smoke also verifies reconciliation and preservation.

The active-attempt pointer and hourly follow-up now use the new configuration
and process identities. Nine gates remain passed; `linux_day` stays false and
the release remains on hold.

### Host sleep invalidates attempt 2, October 16

Attempt 2 failed its continuity check at **05:12:41 UTC on October 16**,
after roughly 21 hours 24 minutes. The harness detected a **49.955-second**
observation gap, exceeding the unchanged five-second limit. macOS power logs
record **Clamshell Sleep at October 15, 22:10:38 Pacific**, followed by repeated
maintenance sleeps, while the campaign's sleep-prevention assertion was active.
The container exited with code 1 and `OOMKilled=false`; this attempt supplies
**no duration credit**. The file/Prometheus phase never started.

The failed report, phase journal, progress, Docker state and bounded host sleep
log are preserved under `attempts/02/results/`, with the review in
`interruption.json`. Source cleanliness and the frozen binary hash still match.
Peak sampled app/helper RSS remained 5.80 MiB, but final counters were not
verified and partial memory observations do not pass the duration gate. There
is no application defect established by this host sleep event.

A separate attempt 3 is prepared with the same frozen source, binary, image and
full-duration scenarios. Its launcher requires AC power and an open lid before
starting. Monitoring waits for those conditions; it does not change host power
settings or restart the failed container. Both 24-hour scenarios remain pending,
`linux_day` stays false, and the first release remains on hold.

### Fresh attempt 3, October 16

After AC power returned and the lid was confirmed open, attempt 3 started at
**18:06:37 UTC on October 16** (11:06 Pacific). It uses the same clean frozen
candidate, ARM64 binary and Docker image in a new container,
`tell-tail-0-2-days-ffda8eaebec5-retry-03`, with separate reports under
`attempts/03/results/`. A fresh three-second preflight verified all 3,000 records
and the exact binary hash; this is startup verification, not duration evidence.
The host watcher and its owned sleep assertion were verified after launch.

Both full 24-hour scenarios start afresh. If uninterrupted, stdin/NDJSON ends
around 18:07 UTC October 17, followed by file/Prometheus around 18:07 UTC
October 18. Neither earlier attempt contributes duration. Nine gates remain
passed, `linux_day` remains false, and publication remains on hold.

### Clamshell sleep interrupts attempt 3, October 16

Attempt 3 failed at **18:51:50 UTC on October 16**, after 45 minutes 12 seconds
of wall time. The observation gap was **93.640 seconds**. macOS recorded
Clamshell Sleep on battery at 11:50:01 Pacific and a wake at 11:51:37, while the
campaign's sleep assertion was active. The container exited with code 1,
`OOMKilled=false`, and the file/Prometheus phase did not start. This attempt has
**zero duration credit**; the completion estimates above no longer apply.

The failed report, progress, phase journal, Docker state, bounded power log and
review are preserved in `attempts/03/results/`. Clean source and binary identity
still match the frozen candidate. Sampled app/helper RSS peaked at 5.64 MiB;
final counters and a complete duration were not verified. No application defect
is established by this host sleep event.

Attempt 4 is prepared separately and waits for AC power and an open lid. The
Mac must retain both conditions throughout the two sequential 24-hour phases.
Monitoring retains the failed attempts and does not change power settings.
Nine gates remain passed, `linux_day` is false, and the release stays on hold.

### Fresh attempt 4, October 16

Attempt 4 started at **23:17:59 UTC on October 16** (16:17 Pacific), after AC
power and an open lid were confirmed. Its new container is
`tell-tail-0-2-days-ffda8eaebec5-retry-04`, with isolated reports under
`attempts/04/results/`. The clean frozen source, ARM64 binary hash and image are
unchanged. A fresh three-second preflight verified all 3,000 records; the host
watcher, owned sleep assertion and container identity were verified after launch.

The stdin/NDJSON and file/Prometheus phases each require a fresh full 24 hours.
If uninterrupted, they finish around 23:18 UTC October 17 and October 18,
respectively. Previous failed attempts remain preserved with zero duration
credit. Nine gates remain passed, `linux_day` is false, and release publication
remains on hold.

### Docker shutdown invalidates attempt 4, October 17

Docker backend logs record its services and API socket shutting down at
**00:29:47 UTC on October 17**. The last minute checkpoint was at 00:28 UTC:
4,201 seconds, 4,200,900 producer records and 5.64 MiB peak sampled app/helper
RSS. No final report was written. Docker is unavailable, so the container's
exit code and OOM state cannot currently be established. The shutdown trigger
is not established either. These partial observations supply **no duration
credit**, and the file/Prometheus phase never started.

The original running journal, host-monitor state, progress and bounded Docker
shutdown log are preserved under `attempts/04/results/`. A separate review
records the interruption, and the stale journal is marked interrupted. Source
and binary identity still match. Only this attempt's host watcher was stopped,
and its owned sleep assertion was verified released after invalidation.

Attempt 5 is prepared with the unchanged candidate, binary, image and two fresh
full-duration scenarios. It requires a responding Docker engine, AC power and
an open lid before launch. Monitoring waits for those prerequisites; it does
not start Docker or alter its settings. If the interrupted owned container
reappears running, the launcher verifies its recorded identity and stops only
that invalidated container before starting the fresh attempt. All four prior
attempts retain zero duration credit. Nine gates remain passed, `linux_day`
remains false, and publication remains on hold.

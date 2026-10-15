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

Correctness, compatibility and distribution readiness are recorded as passed
in the release gate file. Fuzz duration, Linux day runs, dashboard duration,
long-run/default and maximum memory, complete responsiveness and performance
remain open. The candidate workflow's publication job was skipped; the release
is still blocked.

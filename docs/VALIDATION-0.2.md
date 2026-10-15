# Tell-Tail 0.2 implementation validation

October 14, 2026. Implementation steps 1–12 are complete. Release validation
(step 13) is incomplete; the package remains 0.1.0 with Unreleased 0.2 notes.
This report does not certify a GA or release-candidate build.

The committed [machine-readable summary](validation/0.2-smoke.json) records binary
hashes, fixture hashes, configurations, package results, and measurements. Full
local logs/results are in the ignored `work/` directory. Measurements span the
implementation commits and are not a single final-candidate campaign. Reproduce
checks using [TESTING-0.2.md](TESTING-0.2.md).

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
Actionlint 1.7.12 accepts both workflow definitions. Remote CI has not run: this
checkout has no configured Git remote, and no dedicated release runner has been
provisioned.

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

## Gates still open

1. Run 30 minutes per fuzz target and resolve every finding.
2. Run separate 24-hour Linux stdin/NDJSON and followed-file/metrics scenarios.
3. Run one-hour dashboard scenarios on macOS and Linux, including interactions;
   evaluate ten-minute memory medians after warmup.
4. Establish a stable paired performance result below 10% loss, plus final
   candidate responsiveness/resource evidence.
5. Execute remote CI and obtain native runtime evidence for each advertised
   architecture, including Linux x86_64 before shipping that archive.
6. Choose the project license and canonical repository metadata, prepare
   dependency notices and checksummed candidate archives, then validate and bump
   the package/lockfile version together.

The workflows and supervised commands are ready for these runs. No long-duration
job, tag, registry publication, or public release was started by this report.

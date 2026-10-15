# Tell-Tail 0.2 validation commands

Build the exact revision being tested with `cargo build --release --locked`.
The soak runner records the workspace commit and dirty state, binary SHA-256,
workspace toolchain, platform, fixture hash, seed, expected counts, and actual
final counts. An externally supplied binary's compiler is not inferred from the
workspace toolchain.

## Webhook regression checks

```sh
cargo test --locked
cargo build --locked
python3 scripts/check_terminal.py --alerts
```

The Rust suite covers rule transitions, exact-threshold JSON delivery, bounded
HTTP failures, stalled consumers, signal exits, and output modes using local
receivers. The terminal check confirms an alert fires while the display is
frozen and checks restoration after `q`, SIGINT, SIGTERM, and EOF. Both CI
platforms run it. These short fixtures do not replace release-duration or
performance checks with webhook delivery enabled.

## Soak scenarios

```sh
python3 scripts/test_soak_support.py
python3 scripts/soak.py --seconds 60 --profile mixed --report work/mixed.json
python3 scripts/soak.py --seconds 60 --profile burst --tui --report work/tui.json
python3 scripts/soak.py --seconds 60 --input file --profile rotation --output prometheus --rotate-bytes 1048576 --report work/rotation.json
python3 scripts/soak.py --seconds 60 --profile mixed --window 3600 --max-record-bytes 16777216 --rss-limit-mib 128 --report work/maximum.json
```

The sustained profile produces 200-byte text records with an exact 90/9/1
info/warning/error mix. Other profiles include all severities, supported JSON,
unknown text, malformed Unicode/UTF-8, and one oversized record per batch. A seed
controls order and message lengths. Burst traffic includes idle periods.
The `idle` profile sends no records, allowing idle CPU and cancellation checks.

The file rotation profile waits for machine reports before changing generations,
adds a partial record, observes the missing-file state, and checks that the
partial record is discarded on replacement. These barriers make expected counts
well-defined; they do not simulate an exactly-once guarantee for unobserved races.
The TUI soak uses stdin so final counts can be checked without interpreting screen
diffs as acknowledgments. Existing executable tests cover rotation loss limits.

Default input rate is 1,000 records/sec; `--rate 0` disables that rate cap. File
input additionally observes a 10 GiB/day write budget, with up to one batch of
initial burst. `--file-gib-per-day` can increase it for short stress runs. Input
files rotate at 8 MiB by default, and live fixture files are capped at 256 MiB.
The runner drains reports continuously and keeps only the latest snapshot and a
32 KiB terminal/diagnostic tail. A partial line or a replacement transaction may
extend a run slightly beyond its requested duration before shutdown.
`--records N` stops at a fixed workload (a multiple of 100); failing to finish
that workload before the duration limit is an error.

Resource samples include the application and its current descendants. Combined
RSS is conservative because shared pages may be counted more than once. Sampling
once per second does not establish an instantaneous peak. CPU values are sampled
cumulative process times, not exact exit-time accounting; short-lived cleanup
helpers may fall between samples. Descriptor counts are sampled during startup
and then about once per minute, with a 64-descriptor process-tree ceiling.

The observer rejects gaps over five seconds using both monotonic and wall
clocks, and records the maximum observed gap. A sleeping/suspended host or stalled
harness cannot count as uninterrupted release-duration evidence. Interrupted
runs must be investigated and repeated.

Runs retain at most 600 recent RSS samples and compact ten-minute medians. After
ten minutes of warmup, a later median more than max(4 MiB, 20 percent) above the
first post-warmup median fails the plateau check. Default combined RSS must remain
below 64 MiB. Snapshot-age measurements describe the age of published machine
reports at observation, not end-to-end dashboard freshness. Missing observations
are reported as null rather than zero.

Producer CPU uses per-thread CPU time, and write-wall time includes pipe
backpressure. More than 75 percent producer CPU utilization flags a possible
generator limit; a lower value does not prove the producer is unconstrained.
Shutdown timing starts at EOF or the signal request and ends when the monitor
observes exit (20 ms polling resolution). Production and total elapsed times are
reported separately so cleanup overhead remains visible.

Progress goes to stderr each minute. `--report` saves a compact final JSON result;
a failed run saves the failure, latest snapshot, expected counts, and bounded
stderr tail. Durations are capped at 24 hours to bound retained summaries. The
runner checks exact severity/diagnostic/byte totals and cleans up its processes
on failure. Unit tests deliberately perturb expected counts and metric documents
to verify the harness rejects incorrect results.

`python3 scripts/check_metrics_failure.py` removes publication-directory write
permission after a successful snapshot. It checks that the process exits with
status 1 and the preceding complete file remains unchanged. Root container runs
use an unprivileged child so this I/O failure is exercised rather than skipped.

## Release-duration runs

Use a supervised Linux host or a suitable long-running CI runner. Keep progress
logs alongside final results and run competing performance workloads separately.

```sh
python3 scripts/soak.py --seconds 86400 --profile mixed --report work/linux-stdin-24h.json 2> work/linux-stdin-progress.ndjson
python3 scripts/soak.py --seconds 86400 --input file --profile rotation --output prometheus --report work/linux-file-24h.json 2> work/linux-file-progress.ndjson
python3 scripts/soak.py --seconds 3600 --profile burst --tui --exercise-terminal --report work/tui-1h.json
```

Run the one-hour dashboard scenario on both macOS and Linux. The interaction
option cycles through freeze, help, resume, compact size, and full size once per
minute. It records how many actions were sent and still verifies exact final
counts. Screen behavior, stalled-terminal handling, and cleanup are asserted with
`scripts/check_terminal.py` and `scripts/check_terminal_stall.py`; duration alone
does not exercise those interactions. Fuzz commands and independent release
budgets are in [the fuzz workspace](../fuzz/README.md). All release gates and their
required evidence remain in the [0.2 plan](PLAN-0.2.md).

## Performance comparisons

Observe end-to-end dashboard updates with the pinned development-only terminal
emulator ([pyte](https://pyte.readthedocs.io/en/latest/)):

```sh
python3 -m venv work/validation-venv
work/validation-venv/bin/pip install -r scripts/requirements-validation.txt
work/validation-venv/bin/python scripts/check_freshness.py
```

The check writes one record per visible acknowledgment, interprets actual screen
updates at 100 × 26, and enforces p95 ≤500 ms. It also checks freeze/resume, help,
compact unknown/parse-failure counters, and quit within one second. This is a
controlled low-load observation; loaded pipeline measurements remain separate.

```sh
python3 scripts/benchmark.py --report work/benchmark.json
python3 scripts/benchmark.py --suite stream --profile mixed --report work/benchmark-mixed.json
```

Cached scans cover 200-byte text, 500-byte JSON, 16 KiB records, and the seeded
mixed corpus. The first pass warms the file cache; five measured trials follow.
Each fixture has a 512 MiB default disk cap; results record its actual size and
hash. CPU uses waited-child resource accounting. Cached trials include process
startup and final output, and have no live Python producer.

Streaming measurements warm up both modes, calibrate a fixed record count for
approximately five seconds, then run five identical-workload pairs in seeded
random order. The same correctness checks as soaks apply. Reports retain every
trial, throughput range, paired median loss, producer CPU, process-tree samples,
snapshot age, and shutdown latency. Idle JSON/TUI and paced followed-file runs
are separate scenarios. Dashboard freshness still needs a terminal observation
test; a JSON snapshot's age does not establish it.

Throughput pipelines write 64 copies of the 100-record seeded batch at once,
reducing Python bookkeeping and syscall pressure. Initial small-batch trials
hit the producer CPU warning and were inconclusive. Soaks retain one copy by
default; `--batch-copies` is explicit and capped at 64 MiB of producer payload.

A producer CPU fraction above 75 percent or a throughput range wider than
10 percent flags inconclusive measurement conditions. Fewer than five pairs or
shorter than five-second calibration runs are harness smoke tests only. A
qualifying measurement is still not an automatic performance pass: compare
its median dashboard loss with the plan's 10 percent gate and cached rates with
the named reference-host targets. Run without competing builds or fuzz jobs.

The extended CI workflow runs cached scans and five ten-second mixed-input
pipeline pairs on separate native macOS/Linux jobs. Each job finishes compilation
before measuring and retains successful or partial results. A green workflow
alone does not pass the performance gate: inspect generator utilization, timing
spread, absolute rates, and dashboard loss in the report. Hosted-runner variation
can still make a result inconclusive.

## Source package

Run `python3 scripts/check_package.py` from a clean checkout. It runs Cargo's
package build verification, checks the archive contents, extracts into a new
temporary directory, installs with the extracted toolchain pin, and checks a
known mixed fixture using the installed binary. `--allow-dirty` is for developing
the check; its result explicitly records that qualification. Build artifacts and
fuzz corpora are excluded by the package's explicit include list.

## CI tiers

`Checks` runs macOS/Linux correctness, terminal observation, output/backpressure,
Prometheus validation, short soaks, source-package installation, RustSec auditing,
and 30-second fuzz smokes. It records host architecture and enforces resource and
correctness checks; shared-runner throughput is not a release threshold.

`Extended validation` runs weekly or manually: 30 minutes per fuzz target and
one-hour dashboard soaks on both operating systems. Its optional `release_soaks`
input requires a provisioned Linux runner with labels `self-hosted`, `linux`, and
`tell-tail-validation`; the two 24-hour jobs run sequentially with 25-hour job
timeouts. Provision disk space for Rust builds in addition to the 256 MiB fixture
budget. No dedicated runner is provisioned by this repository. Workflow files
alone are not evidence that these jobs have passed.

Actions are pinned to upstream checkout v4.3.1 and upload-artifact v4.6.2 commits.
Promtool 3.3.0 downloads are checked against pinned upstream SHA-256 digests.
The package uses Rust 1.88; only fuzz tooling requires the pinned nightly.

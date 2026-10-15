# Tell-Tail 0.2 validation commands

Build the exact revision being tested with `cargo build --release --locked`.
The soak runner records the workspace commit and dirty state, binary SHA-256,
workspace toolchain, platform, fixture hash, seed, expected counts, and actual
final counts. An externally supplied binary's compiler is not inferred from the
workspace toolchain.

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

Resource samples include the application and its current descendants. Combined
RSS is conservative because shared pages may be counted more than once. Sampling
once per second does not establish an instantaneous peak. CPU values are sampled
cumulative process times, not exact exit-time accounting; short-lived cleanup
helpers may fall between samples. Descriptor counts are sampled during startup
and then about once per minute, with a 64-descriptor process-tree ceiling.

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

## Release-duration runs

Use a supervised Linux host or a suitable long-running CI runner. Keep progress
logs alongside final results and run competing performance workloads separately.

```sh
python3 scripts/soak.py --seconds 86400 --profile mixed --report work/linux-stdin-24h.json 2> work/linux-stdin-progress.ndjson
python3 scripts/soak.py --seconds 86400 --input file --profile rotation --output prometheus --report work/linux-file-24h.json 2> work/linux-file-progress.ndjson
python3 scripts/soak.py --seconds 3600 --profile burst --tui --report work/tui-1h.json
```

Run the one-hour dashboard scenario on both macOS and Linux. Interactive key,
resize, stalled-terminal, and cleanup behavior are tested separately with
`scripts/check_terminal.py` and `scripts/check_terminal_stall.py`; duration alone
does not exercise those interactions. Fuzz commands and independent release
budgets are in [the fuzz workspace](../fuzz/README.md). All release gates and their
required evidence remain in the [0.2 plan](PLAN-0.2.md).

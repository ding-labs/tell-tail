# Continuous webhook validation

`scripts/soak_webhooks.py` runs the real Tell-Tail binary against deterministic
mixed text/JSON logs. It captures each HTTP webhook, checks its statistics and
transition order, and optionally formats and forwards it to a Slack incoming
webhook. The relay belongs to the test harness; the binary's generic JSON payload
is unchanged. Slack requires its own message shape, such as a `text` field.

## Default 24-hour scenario

- 1,000 records/second, a 60-second rolling window, 25% error threshold,
  100-record minimum and the normal 300-second breach cooldown.
- Healthy traffic contains 1% errors. Each hour has a two-minute spike starting
  two minutes into the hour, with 40% errors and 10% fatals, followed by recovery.
- Expect exactly 24 breach notifications and 24 recovery notifications, in order
  and within a single monitoring session. The receiver acknowledges each with
  HTTP 204 after Slack accepts it. Slack failures fail the run.
- The monitor/helper RSS ceiling is 64 MiB. Sampling is approximately once per
  second; ten-minute memory medians must plateau after warmup. Exact final log
  counters and byte counts are checked independently against generated input.
- Observation gaps over five seconds invalidate continuity, including host sleep.
  Interrupted time is never combined with another attempt's duration.

Run inside a Linux container pinned to one logical CPU with a one-core CPU quota,
`--memory 1g --memory-swap 1g`, and the usual source, binary, harness and result
mounts. This imposes a 1 GiB limit with no swap on the whole container, including
the generator and relay. It measures the application under resource limits;
it does not include a separate VM operating system's memory footprint.

```sh
python3 /harness/soak_webhooks.py \
  --binary /candidate-bin/tell-tail \
  --source-directory /source \
  --source-commit FULL_FROZEN_COMMIT \
  --results /results/day \
  --slack-webhook-file /run/secrets/slack-webhook \
  --require-container-limits
```

Keep the Slack webhook URL in a mode-0600 file and mount it read-only. Do not put
it in command arguments, reports, Git, or progress logs. The receiver listens
only on container loopback; no host port needs to be published. Use a new result
directory for each attempt; an existing directory is rejected.

## Preflight and evidence

Before starting the day, use the same binary, container limits and Slack channel
with `--seconds 20 --cycle-seconds 20 --breach-start 5 --breach-seconds 5 --window 2
--label preflight`. This checks both notification paths without waiting an hour.
It provides no credit toward the full-duration test.

Results include `campaign.json`, `report.json`, `webhooks.ndjson`, and the normal
minute progress stream. Reports retain the source commit, binary SHA-256,
exact counters, memory observations, notification count and kernel cgroup limits.
Retain the harness revision/file hashes and Docker configuration alongside them.
The host watcher distinguishes a completed report from an unexpected container
exit and preserves interrupted journals.

On a laptop, verify AC power and an open lid before launch. An owned sleep
assertion prevents idle sleep while the host watcher runs; it does not make
closing the lid or losing the Docker engine valid continuous-test conditions.

The October 22 scenario freezes application source
`328a0994b52bd6100f9a4419fbee66c120c02037`. Its Linux ARM64 release binary SHA-256 is
`34a3e0b8854f3a4c7579004f90c08ed7db72b4d9d20824f07848d8768e95479f`.
The constrained preflight passed with 20,000 records, two Slack notifications and
5.06 MiB peak sampled monitor/helper RSS. It supplies no duration credit to the
separately reviewed day run below.

The day run started October 22, 2026 at 10:33:49 PDT and finished October 23 at
10:33:50 PDT. Its container is
`tell-tail-webhooks-day-2026-10-22`; the frozen manifest and evidence are under
`work/soak-2026-10-22-1gb1cpu/`. The start check verified the resource limits,
running journal, host watcher and owned sleep assertion. Notifications and
start/final summaries go to
[#telltale-soak-2026-10-22](https://app.slack.com/client/T0ARKPDHAKX/C0CC7NJS7A4)
in devrel-flywheel.

## Reviewed October 23 result: passed

An independent review reconstructed both seeded input fixtures, recomputed the
exact totals and memory plateau checks, checked every event and its session,
and corroborated the report with Docker exit evidence, the retained minute
progress stream and Slack channel history.

| Check | Reviewed evidence |
| --- | --- |
| Continuous production | 86,400.000516 seconds; maximum observation gap 0.180433 seconds |
| Input and final counters | 86,400,000 records; 16,850,275,200 bytes; exact agreement |
| Levels | 75,772,800 info; 8,352,000 warn; 1,987,200 error; 288,000 fatal; all other levels zero |
| Parsing and shutdown | No parse failures, oversized, pending, skipped or discarded records/bytes; final state complete |
| Webhooks | 24 ordered breach/recovery pairs, one unchanged session; all 48 accepted by Slack and visible in the channel |
| Monitor/helper memory | Peak sampled RSS 7.953125 MiB; 143 ten-minute medians between 4.460938 and 4.765625 MiB; post-warmup limit 8.460938 MiB |
| Producer CPU | 171.892329 CPU seconds / production duration = 0.001989495 of one CPU, below 0.75 |
| Container enforcement | CPU 0, quota 100000/100000; 1,073,741,824-byte memory limit; zero swap; read-only root; peak cgroup memory 42 MiB |
| Exit and identity | Exit 0, zero restarts, OOMKilled false and zero cgroup OOM events; frozen source, binary and harness hashes unchanged |

AC power disconnected during the final hour. The user explicitly authorized
continuing on battery; the lid stayed open at follow-up checks and the owned
sleep assertion remained present until the watcher finished. The continuity
check passed without a restart or combined partial durations.

The separate `results/day/report.json`, `results/day/webhooks.ndjson`, final
Docker state/logs and Slack review are preserved under the campaign directory.
`results/reviewed-result.json` records the reviewed values and evidence SHA-256
manifest; `review_result.py` preserves the independent checks. The runner's
final Slack summary was confirmed, so no duplicate was sent.

This pass covers the frozen webhook build in a Linux ARM64 container under the
one-CPU/1-GiB/no-swap limits. It excludes a separate VM operating system and
does not certify the older frozen release candidate or change its release gates.

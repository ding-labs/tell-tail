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
5.06 MiB peak sampled monitor/helper RSS. The full-day verdict requires review
of its separate completed report.

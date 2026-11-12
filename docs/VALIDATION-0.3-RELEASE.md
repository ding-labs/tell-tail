# Tell-Tail 0.3.0 release validation

Candidate: `05b3376e2ba1f809bf0a789faa477f3f17e1bcf2`. Version 0.3.0 includes
the merged demo, expanded parser coverage, dashboard interpretation features,
and recovery/rotation/shutdown fixes. Application changes are frozen at this
commit; subsequent validation records and plans do not change the candidate.

Publication is pending. No gate is credited from the 0.2.0 campaign. The required
checks below must finish and their exact-source reports must be reviewed before
`packaging/validation.json` approves publication.

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

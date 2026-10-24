# Telltale 0.2.0 release candidate 1

The annotated tag `v0.2.0-rc.1` freezes source
`33f2f9ac959350bcd72332f5a62b53480361c933`, with package version `0.2.0`.
The tag does not authorize publication. Later harness and evidence commits do
not change this source identity.

The distribution PR [#7](https://github.com/zuchka/tell-tail/pull/7) and tap PR
[#3](https://github.com/ding-labs/homebrew-tap/pull/3) were merged with merge
commits, preserving their individual commits. The previous candidate's gate
record is archived under `packaging/validation-history/`; its passes do not
transfer to this candidate.

## Reviewed checks

[Checks](https://github.com/zuchka/tell-tail/actions/runs/40055442773) passed for
this exact source. This covers the native correctness and compatibility jobs,
including CLI, schema, exit status and Prometheus fixtures.

[Release candidate](https://github.com/zuchka/tell-tail/actions/runs/40055442749)
passed for this exact source: three native archives, temporary Homebrew installs,
Linux static-link checks, Debian 11 and Alpine smoke tests, and source package
checks. The assembled manifest and complete `SHA256SUMS` were verified locally.
The x86_64 Linux archive SHA-256 is
`5e8d892e4772edbf180736f8bd028fc39a15d2990c4a738e69d937aef8cc05e4`;
its executable SHA-256 is
`a977f38fb27f54ffc7bfff36fa5c8c626090949e3bd86249d594cafe4343f678`.

## Pending checks

Fresh [extended validation](https://github.com/zuchka/tell-tail/actions/runs/40055922040)
and [performance](https://github.com/zuchka/tell-tail/actions/runs/40055923758)
were dispatched for this source. Results require review before updating gates.
The Linux day gate requires separate uninterrupted 86,400-second stdin/NDJSON
and followed-file/Prometheus rotation scenarios, sequentially. Earlier partial
campaigns supply no duration credit.

The user selected Railway for the new Linux campaign and authorized removal of
its temporary infrastructure after collecting the results. Apple signing is
tracked in [#9](https://github.com/zuchka/tell-tail/issues/9); the user's crates.io
account and publishing setup is tracked in [#12](https://github.com/zuchka/tell-tail/issues/12).

## Railway campaign started

The [temporary Railway service](https://railway.com/project/e4d336bc-70a2-4c56-9f34-18f173f86d82/service/8b961134-82c8-4a11-ba0b-32e6c5d78b89)
started the stdin/NDJSON day phase on October 24, 2026 at 03:03:39 UTC
(October 23, 8:03:39 pm Pacific). If both phases remain uninterrupted, the
campaign should finish around October 26 at 03:04 UTC (October 25, 8:04 pm
Pacific). This is a start record, not a duration pass.

Deployment `a475e266-ec95-4f14-956f-93120cd92780` uses pinned Debian 12/Python
3.11 and the verified x86_64 musl executable above. Runtime cgroup evidence
confirms a one-CPU quota, 999,997,440-byte memory ceiling and zero swap.
The service has one replica, restart policy NEVER, sleep disabled and no
GitHub source connection. Its `/results` volume retains the journal/reports.

Both three-second preflights passed exact counts and bytes, clean source and
harness provenance, binary identity, resource limits and responsiveness.
Sampled app/helper RSS was 4.93 MiB for stdin and 3.20 MiB for the followed file;
maximum observation gaps were below 32 ms. These checks earn no day-run credit.
The initial journal, runtime limits and complete preflight reports are under
`docs/validation/0.2.0-rc.1/`.

A follow-up checks this campaign every 30 minutes, reports meaningful changes,
and collects/checksums final or failed evidence before deleting the exact
owned project, service, deployments, generated domain and volume. It must verify
cleanup and retain the evidence. The former local-candidate follow-up was
replaced so it cannot restart the obsolete campaign. The separate webhook
validation remains untouched. Final Linux gate review is still pending.

## Reviewed fuzz results — October 24

All three sanitizer-backed targets completed 1,801 seconds on the exact clean
RC1 source in [extended validation](https://github.com/zuchka/tell-tail/actions/runs/40055922040),
with no finding: framing 29,621,664 executions, classification 62,162,142, and
timed engine 2,971,822. The pinned nightly, bounded input/RSS settings and log
hashes are retained in `docs/validation/0.2.0-rc.1/fuzz-summary.json`; full logs
were downloaded into the ignored `work/rc1-ci/extended` evidence directory.
The fuzzing gate passes. The dashboard-hour jobs are still running.

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

## Release status

Fresh [extended validation](https://github.com/zuchka/tell-tail/actions/runs/40055922040)
and [performance](https://github.com/zuchka/tell-tail/actions/runs/40055923758)
have been reviewed below. Nine gates now pass, including both Linux days.
Performance fails for RC1, so publication remains blocked.
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
The fuzzing gate passes. Dashboard-hour results are reviewed below.

## Performance blocker — October 24

The fresh [controlled Linux measurement](https://github.com/zuchka/tell-tail/actions/runs/40055923758)
completed correctly but **fails the unchanged performance gate**. On
`runnervm8k3b6`, AMD EPYC 7763, producer CPU 0 and consumer/helper CPU 3,
five paired runs each processed 404,262,400 identical mixed-input records.
Median headless/TUI rates were 6.78M/3.27M records/s: paired median dashboard
loss **51.83%**, above the required <10%. Headless/TUI timing ranges were
2.02%/0.53%, and maximum producer CPU use was 36.62%, so this is a qualifying
measurement rather than a producer or timing-noise rejection. The cached text
median of 20.64M records/s exceeds the absolute 1M reference target.

The separate Linux extended run corroborates the slowdown (51.25% loss, stable
ranges). macOS measured 46.73% loss, but its 14.73% headless range makes that
measurement inconclusive. A green workflow means the benchmark completed;
it does not mean the performance gate passed. Report hashes, exact source and
binary identities, fixture hashes, all pair timings and reference hosts are in
`docs/validation/0.2.0-rc.1/performance-summary.json`; complete reports and logs
were downloaded under ignored `work/rc1-ci`.

The new interactive error-preview capture path is under investigation. The
frozen source, tag and running Railway binary remain unchanged. Linux duration
validation continues, and publication remains blocked by performance and the
other pending gates.

### Merged preview fix

[PR #14](https://github.com/zuchka/tell-tail/pull/14) removes per-record string
allocation/sanitization from preview capture, using reusable bounded slots and
formatting the retained sample only during publication. Its local five-pair
comparison reduced median dashboard loss from 57.32% to 3.02%, with qualifying
noise/producer conditions. The full Rust tests and clippy pass. Controlled Linux
run [40059139178](https://github.com/zuchka/tell-tail/actions/runs/40059139178)
passed for code commit `327d5bcb395ef3aece1786cfbc2eabf0f7fdaf38`, with
3.76% median dashboard loss and qualifying timing/producer conditions. The
performance fix was merged on October 24 at the user's request, using merge
commit `ca10c1372975afb80363b27d4b66e6cfa545c0c4`. All five individual PR
commits were preserved, and all final head checks passed. Its full measurements
are retained in that PR. The existing Railway deployment remains healthy and
unchanged; its results will be preserved and reviewed before cleanup.
These results apply to the merged fix, not RC1. The RC1 source, tag, Railway
soak and failed performance gate remain unchanged.

## Reviewed hour and responsiveness results — October 24

The exact clean RC1 source completed native one-hour burst/TUI runs on macOS
ARM64 and Linux x86_64 in [extended validation](https://github.com/zuchka/tell-tail/actions/runs/40055922040).
Both passed exact counters and bytes, all 360 terminal actions, continuity and
shutdown checks. Combined app/helper RSS is sampled once per second.

| Observation | macOS ARM64 | Linux x86_64 |
| --- | ---: | ---: |
| Production duration | 3,600.121 s | 3,600.004 s |
| Records verified | 3,596,900 | 3,597,000 |
| Peak sampled RSS | 7.05 MiB | 9.18 MiB |
| First post-warmup ten-minute RSS median | 6.33 MiB | 9.16 MiB |
| Largest later median | 6.38 MiB | 9.18 MiB |
| Maximum observation gap | 232 ms | 30 ms |
| Shutdown observed | 256 ms | 31 ms |

The default-memory ceiling and post-warmup plateau checks pass. The two day
scenarios remain separate requirements; these hour runs supply no day credit.
The maximum headless configurations (window 3600, 16 MiB record limit) also
passed 60 seconds, peaking at 37.20 MiB and 27.72 MiB respectively. The maximum
memory gate was held pending the fixed-geometry terminal check; the exact-source
follow-up below now completes that requirement.

The [exact-source native correctness run](https://github.com/zuchka/tell-tail/actions/runs/40055442773)
retained 25 real write-to-visible-count observations per platform at 100 × 26.
p95 freshness was 361 ms on macOS and 220 ms on Linux, below the 500 ms limit;
quit completed in 290 ms and 213 ms. The controlled terminal, signal, stalled
output and backpressure checks passed in the same run; extended loaded pipeline
and hour-run shutdown checks also pass. Responsiveness, default memory and the
one-hour dashboard gates now pass. Full report hashes and compact observations
are in `docs/validation/0.2.0-rc.1/dashboard-hour-summary.json`; downloaded raw
reports remain under ignored `work/rc1-ci`.

## Reviewed maximum settings — October 24

[Resource-only run 40061097053](https://github.com/zuchka/tell-tail/actions/runs/40061097053)
passed both output modes on native macOS ARM64 and Linux x86_64, with window
3600 and a 16 MiB record limit. Every scenario produced 6,000 records and
1,007,974,560 bytes over at least 60 seconds, matching all exact counters.
Fixed-geometry TUI peaks were 31.50 MiB and 28.48 MiB respectively; the fresh
headless peaks were 38.89 MiB and 28.12 MiB. Combined app/helper RSS was sampled
once per second and stayed below the unchanged 128 MiB ceiling. Continuity,
producer utilization and shutdown checks passed.

The workflow definition came from metadata commit `9c38ba4`, while its explicit
checkout built clean frozen source/harness `33f2f9a`. Both executable hashes
match the earlier hour/resource run. The new workflow mode skips repeated fuzz,
performance and one-hour work, adds the previously missing terminal maximum
case, and retains the same case in ordinary future extended campaigns.
Complete report hashes and configurations are retained in
`docs/validation/0.2.0-rc.1/maximum-memory-summary.json`; downloaded reports are
under ignored `work/rc1-ci/maximum-config`.

Eight RC1 gates pass. The Linux day campaign is still running and RC1's
performance failure remains a release blocker. PR #14's validated performance
fix cannot pass a gate for the unchanged RC1 binary. The browser test launcher
fix from that PR is also applied to this evidence branch after the same startup
timeout occurred here; the frozen application and remote campaign are unchanged.

## Reviewed Linux stdin day — October 25

The stdin/mixed/NDJSON scenario completed 86,400.000 seconds on the unchanged
RC1 deployment. Independent review reconstructed the frozen fixture's expected
counters and bytes: all 86,400,000 records and 925,288,704,000 bytes match, with
no pending data. Clean source/harness provenance, the executable hash and the
completed report's journal hash were verified.

Combined app/helper RSS peaked at 4.95 MiB across 85,347 resource samples.
All 142 complete post-warmup ten-minute medians pass the plateau check: the
baseline was 4.48 MiB and the largest later median was 2.29 MiB. The maximum
observation gap was 231 ms, p95 snapshot age was 140 ms, and producer CPU use
was 0.75%. The configured one-CPU/1 GB limits and runtime cgroup limits match;
a read-only cgroup check after the phase found zero OOM or OOM-kill events.
The same deployment continues with restart disabled and sleep disabled.

The evidence bundle, journal, complete report/stdout/progress, runtime evidence
and all 1,445 platform log lines spanning this phase were preserved locally.
Their checksums and reviewed measurements are retained in
`docs/validation/0.2.0-rc.1/stdin-day-summary.json`. The followed-file/rotation/
Prometheus scenario began October 25 at 03:03:39 UTC and remains healthy.
The Linux day gate stays pending until this second full 86,400-second scenario
finishes and passes independent review. No infrastructure has been removed;
final evidence collection and authorized cleanup remain outstanding.

## Reviewed Linux followed-file day — October 25

The followed-file/rotation/Prometheus scenario completed 86,400.041 seconds
on October 26 at 03:03:40 UTC (October 25, 8:03:40 pm Pacific). Independent
review reconstructed the frozen fixture and its rotation boundaries: all
1,002,700 records and 10,738,294,067 bytes match, including 1,253 discarded
partial records and 18,795 skipped bytes across 1,253 rotations. Pending bytes
are zero. The final report matches stdout and its journal checksum. A final
read-only runtime inspection confirms clean frozen source/harness and the
unchanged executable hash.

Combined app/helper RSS peaked at 3.28 MiB across 85,347 resource samples.
All 142 complete post-warmup ten-minute medians pass: the baseline was
3.21 MiB and the largest later median was 1.88 MiB. Maximum observation gap
was 242 ms, p95 snapshot age was 110 ms, and producer CPU use was 0.13%.
Final cgroup counters show zero OOM or OOM-kill events; the one-CPU,
999,997,440-byte runtime memory ceiling and zero swap match the configured
one-CPU/1 GB service. The deployment stayed unchanged, with one replica,
restart disabled, sleep disabled and no GitHub autodeploy.

Both separate uninterrupted 86,400-second scenarios now pass the Linux day
gate. The complete 15-file archive, final source/binary inspection, runtime
limits, journal and all 2,889 platform log lines were preserved in ignored
`work/rc1-railway-evidence/20261026T032137Z-final/`. Archive SHA-256 is
`34723c078ca86b85054931d7bc92031a64b47f00117b026623c12c1730efc0fb`.
Reviewed measurements and evidence hashes are retained in
`docs/validation/0.2.0-rc.1/file-day-summary.json`; the unchanged first report
was checked again against its previously reviewed checksum. Cleanup follows
only after this evidence is verified safe.

Nine of ten RC1 gates pass. The performance gate remains failed for frozen
`33f2f9a`; PR #14's separately validated fix does not change this candidate's
identity or approve its release. No new candidate or publication is started.

## Cleanup awaiting Railway account confirmation

The complete archive and journal passed independent review before cleanup.
The explicit project-delete command for `e4d336bc-70a2-4c56-9f34-18f173f86d82`
(`telltale-rc1-soak`) was rejected: Railway requires the account's two-factor
confirmation in interactive mode, or a two-factor code. No deletion was
accepted, and the project, service, deployments, domain and volume remain.
The user has been asked to complete that account confirmation for this exact
temporary project. The monitor will verify resource removal before claiming
cleanup is complete; it must not bypass the authentication requirement.
The saved `cleanup-status.json` records the rejected attempt. Full evidence,
the initial failed build, and source/binary context remain preserved locally.

## What remains before 0.2.0 publication

- Freeze a candidate containing the merged performance fix and finish the
  required validation for that exact source and its binaries. RC1 now passes
  nine gates, but its qualifying 51.83% dashboard loss remains a failure.
  The fix's 3.76% Linux result covers a different source and cannot approve
  RC1 or replace the new candidate's complete release record. Finalize its
  versioned changelog/release notes and review its clean source package.
- Record every passing gate with reviewed evidence for the selected candidate
  on `main`. The release workflow must still reject this RC1 record because
  performance is false. The protected `release` environment is configured to
  require `zuchka`'s approval; publication remains a separate authorized action.
- For crates.io, complete the intended owner account, verified email, package
  name/ownership check and interactive publishing authentication in
  [#12](https://github.com/zuchka/tell-tail/issues/12), which remains open with
  setup unconfirmed. Repeat the package dry run on the validated candidate
  before the separately authorized first upload, then test a clean registry
  install. Manual first publication needs no GitHub Actions token. This account
  setup applies to the crate upload; GitHub Releases and Homebrew use their own
  prepared workflows.
- After an approved stable GitHub release is public, import and verify its
  formula through the shared tap workflow. Native three-platform archives,
  static Linux compatibility checks, checksums/notices and temporary Homebrew
  installation are already prepared and tested.

Developer ID signing and notarization remain the explicitly deferred follow-up
[#9](https://github.com/zuchka/tell-tail/issues/9). Homebrew is the primary Mac
installation path. Signing and the clean browser/quarantine installation check
must be addressed before promoting direct macOS downloads; this is not an
additional Linux or Homebrew release gate. No extra campaign, new tag, crate,
release or formula has been published by this monitoring work.

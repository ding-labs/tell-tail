# Tell-Tail 0.2.0 publication validation

The source is frozen at `02a6476f9921ba45c59a6ce98bd68b317bac79b6`, the
exact Git commit already published as crates.io 0.2.0. The release tooling and
validation-record commits on main do not change that candidate. No existing
version or candidate tag is moved. Publication remains blocked until every gate
in `packaging/validation.json` passes for this source.

## Reviewed regular checks

[Checks 40619394057](https://github.com/ding-labs/tell-tail/actions/runs/40619394057)
passed on this exact source. The retained
[CI summary](validation/0.2.0-release/ci-summary.json) records package/install and
freshness observations plus hashes of all downloaded platform reports. Correctness,
compatibility, controlled responsiveness and distribution are recorded as passed.
Linux/macOS visible-count p95 is 215/399 ms; quit is 212/383 ms. Source package
installation, embedded dashboard assets and web-server shutdown passed on both.

[Native release build 40619393915](https://github.com/ding-labs/tell-tail/actions/runs/40619393915)
passed macOS ARM64 and both static Linux architectures, including temporary
Homebrew installations. Its downloaded release manifest and all archive
checksums were reverified before selecting the Linux binary for the new soak.

## Remaining checks launched October 28, 2026

- [Extended validation 40789256596](https://github.com/ding-labs/tell-tail/actions/runs/40789256596)
  runs the candidate's three 30-minute fuzz targets, one-hour interactive Linux
  and macOS dashboards, maximum-memory cases and performance scenarios.
- [Controlled performance 40789260268](https://github.com/ding-labs/tell-tail/actions/runs/40789260268)
  builds the frozen candidate using the current main harness and separate
  producer/consumer CPU placement. The report's candidate and harness identities
  must both be retained. A green run alone is not a qualifying performance result;
  inspect all noise, producer, cached-rate and paired-loss measurements.
## Explicit maintainer waiver of the two full-day scenarios

On October 28, 2026, the user explicitly instructed: “Skip the two 24 hour soaks
and just move forward.” This waives only the `linux_day` gate for version 0.2.0,
source `02a6476f9921ba45c59a6ce98bd68b317bac79b6`. The gate remains `passed: false`;
its candidate-specific `waivers.linux_day` record carries the maintainer, time,
reason, version and full source SHA. No other gate is waived. The release notes
will disclose the exception instead of claiming a full validation pass.

The temporary Railway campaign was stopped after preserving its partial
synthetic reports and checksums under `work/release-0.2.0/waived-linux-campaign/`.
The service has no active deployment; its result volume is retained. Neither
startup preflights nor the short partial run supply any 24-hour duration credit.
The historical [startup record](validation/0.2.0-release/linux-start.json) retains
resource/source/binary identities; it does not describe the current stopped state.
The canceled day scenarios will not be restarted for this release.

The shorter, still-required checks continue. The exact-source maximum-configuration
macOS TUI run passed for 60 seconds at window 3600 and a 16 MiB record limit:
6,000 exact records, 26.70 MiB sampled app/helper RSS, 109 ms maximum observation
gap. Its [full report](validation/0.2.0-release/maximum-tui-macos.json) retains
configuration, source/harness/binary identity and resource observations. It does
not replace the one-hour default dashboard memory runs.

Full startup evidence, uploaded deployment context, original native archives,
and downloaded CI reports are preserved under ignored `work/release-0.2.0/`.
The earlier validation record is retained in
[previous-validation.json](validation/0.2.0-release/previous-validation.json).
Old-candidate results are historical evidence, not passes for this candidate.

## Completion and publication

Download and preserve final reports, progress logs, deployment logs and checksums.
Review exact identities, complete durations and every gate before marking passes.
Do not shorten a required run or combine interrupted durations. Resolve a genuine
failure and repeat the affected validation; never mark it passed from another
candidate's results.

After all nine unwaived gates pass and the explicit day waiver is recorded,
merge the reviewed evidence onto main and dispatch the
unified `release.yml` workflow with `publish=true` and the full frozen source SHA.
The user authorized publication after validation in this chat on October 28.
The protected release environment still applies. The already published crates.io
version must be verified and reused; GitHub Releases and Homebrew are the remaining
channels. Verify successful tap import and user installation commands before
reporting completion. The canceled day deployment is already stopped; preserve its result volume
and unrelated projects.

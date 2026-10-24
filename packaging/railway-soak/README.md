# Temporary Railway Linux validation

This orchestration runs the clean source and existing `scripts/soak.py` from
`v0.2.0-rc.1`; it does not rebuild or modify the candidate. The x86_64 static
Linux binary comes from the exact-source GitHub Actions release artifact.

Create an isolated upload context with these three files plus the verified
archive's `tell-tail`, `BUILD.json`, and an `identity.json` containing
`source_commit`, `binary_sha256`, `artifact_run`, and `archive_sha256`.
Verify the complete release manifest and checksums before extracting. The
Dockerfile source commit and identity must match the frozen candidate.

Use a dedicated temporary project with one service, one `/results` volume,
1 vCPU and 1 GB RAM, sleep disabled, restart policy NEVER and no GitHub source
connection. Deploy only the isolated context. Confirm service settings, actual
container limits and both three-second preflights before reporting the run as
started. Each phase then runs for a fresh 86,400 seconds. Neither preflight nor
an interrupted attempt earns duration credit. A persisted journal prevents an
accidental redeploy from starting the campaign again.

Only synthetic evidence is exposed through `/campaign.json` and
`/evidence.tar.gz`; `/health` provides readiness. No credentials belong in the
context or service. The application is never configured to send alerts.
The bounded evidence server stays up after completion so the cleanup follow-up
can collect reports even if the local computer was asleep at completion.

On success or failure, download and checksum the evidence bundle, preserve it
locally, and independently inspect the journal and both phase reports. Require
exact counters and bytes (checked by the frozen harness), unchanged identity,
full production durations, observation gaps at most five seconds, default RSS
below 64 MiB, stable post-warmup memory medians and an unsaturated producer.
Only then update the exact-source release gates. A server restart with a running
journal marks interruption; never resume partial durations.

After evidence collection, delete only this campaign's recorded project ID,
which removes its service, deployments, generated domain and volume. Verify the
project is absent and record cleanup. If an infrastructure crash prevents
collection, preserve logs first; a retrieval-only redeploy may serve the retained
volume, but must never rerun a campaign. Keep publication on hold. The scheduled
follow-up must remain quiet while healthy state is unchanged and must stop after
review and cleanup are complete.

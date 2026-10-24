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

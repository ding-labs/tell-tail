# Repository working preferences

## Long-duration soak tests

The two 24-hour soak scenarios (stdin/NDJSON and followed-file/metrics) are optional, non-blocking background validation in this repository and any other repository. Never make their completion or success a release gate, required CI status, or dependency of the publish/deploy workflow. They must not delay continuous delivery or require a per-release waiver to proceed. Preserve and review their results and report actionable failures separately. Only make them blocking if the user explicitly requests that exception.

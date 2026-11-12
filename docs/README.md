# Tell-Tail documentation

Read the [hosted released guides](https://docs.ding.ing/tell-tail/) or the
[hosted unreleased preview](https://docs.ding.ing/tell-tail/next/) on
[Ding docs](https://docs.ding.ing/). This directory is the source of truth;
hosted builds use pinned revisions and may lag this checkout.

Tell-Tail helps you understand what one process or log source is doing now.
Start with the [installation and quick start](../README.md#install-and-try-it).
The built-in demo, expanded parsers, parsing coverage, severity history, and
activity freshness require **0.3.0 or later**. Their guides use the installed
`tell-tail` command. MCP diagnostics require **0.4.0 or later**.

## Using Tell-Tail

| Guide | What it answers |
| --- | --- |
| [MCP diagnostics](MCP.md) | Existing desktop MCP logs, grouped failures, live timeline, safe evidence, source coverage, demo, and API |
| [Log format recognition](LOG-FORMATS.md) | Automatic styles, one-flag profiles, nested keys, container wrappers, parser hints, and precise limits |
| [Understanding a live log stream](MONITORING.md) | What coverage, histories, ratios, and activity ages mean; worked examples, parser conventions, time scopes, and controls |
| [Built-in demo](DEMO.md) | One-command browser start, phase-by-phase parsing walkthrough, flag examples, switching to real logs, limits, and shutdown |
| [Web dashboard](WEB-DASHBOARD.md) | Browser display, freeze, exact values, connection states, accessibility, and browser opening |
| [Operations and troubleshooting](OPERATIONS.md) | Empty input, partial lines, unknown severity, stale displays, rotation, diagnostics, and service setup |
| [Apple unified logs](APPLE-UNIFIED.md) | Native macOS capture and saved NDJSON, severity mapping, skipped metadata, and lifecycle |
| [Webhook alerts](../README.md#webhook-alerts) | Thresholds, minimum samples, recovery, cooldown, formats, delivery bounds, and examples |

Coverage measures recognized severity. Activity measures received records.
Neither establishes application health. Histories and bounded error previews
remain in memory; these features do not add persistent log retention, forwarding,
or an inference service.

## Integrating and developing

| Guide | What it defines |
| --- | --- |
| [Web frame contract](WEB-FRAMES.md) | Exact field types, optional additions, coverage formulas, validation, compatibility, sequence handling, and bounds |
| [Snapshot API](SNAPSHOTS.md) | Owned snapshots, coalescing notifications, presentation fields, and serialization |
| [Web server](WEB-SERVER.md) | Loopback endpoints, stream limits, access checks, and shutdown |
| [Output isolation](OUTPUT.md) | Bounded output, backpressure, cancellation, and failure handling |
| [Dashboard validation](TESTING-DASHBOARDS.md) | Reproducible automated checks, feature coverage, manual acceptance, and evidence expectations |
| [Broader validation commands](TESTING-0.2.md) | Fuzzing, performance comparisons, packaging, and optional background soaks |
| [Distribution and releases](RELEASING.md) | Build/install artifacts, supported platforms, and publishing procedures |
| [Design](DESIGN.md) / [Dependencies](DEPENDENCIES.md) | Visual conventions and dependency choices |

## Plans and historical evidence

The [passive MCP watcher plan](PLAN-MCP.md) records the implemented
`tell-tail mcp --open` workflow, supported-source boundaries, implementation
increments, and acceptance checks. Its [local log audit](MCP-LOG-AUDIT-2026-11-12.md)
records the evidence for proceeding. [Implementation validation](VALIDATION-MCP.md)
records measurements and remaining external acceptance work.

[Linux log inputs](PLAN-LINUX-INPUTS.md) defines the proposed journal parser,
native system/user/kernel capture, later kernel-device support, and acceptance
checks. These are planned features, not flags available in 0.3.0.

[HTTP status monitoring](PLAN-HTTP-STATUS.md) defines access-log and structured
status parsing, independent HTTP metrics, severity policy, bounded storage,
output compatibility, and alerting. It is also planned work after 0.3.0.

The [state-machine inventory and implementation plan](PLAN-STATE-MACHINES.md)
maps observable transitions to their owners and tests, starting with dashboard retry behavior.

[Original scope](PLAN.md), [0.2 reliability plan](PLAN-0.2.md), and
[web delivery plan](PLAN-WEB.md) describe milestones and their decisions. Read
them alongside the [changelog](../CHANGELOG.md), not as a list of features that
are necessarily still pending.

The [0.3 release validation](VALIDATION-0.3-RELEASE.md),
[0.2 release validation](VALIDATION-0.2-RELEASE.md),
[0.2 implementation validation](VALIDATION-0.2.md),
[initial validation](VALIDATION.md), and dated records in
[validation/](validation/) apply to the revisions and environments they name.
Later changes need their own appropriate checks. The two 24-hour stdin/NDJSON
and followed-file/metrics soaks are optional background observations, never
release or publication prerequisites; retain results and review actionable
failures separately.

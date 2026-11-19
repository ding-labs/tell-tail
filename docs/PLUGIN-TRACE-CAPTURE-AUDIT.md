# Plugin trace capture audit

November 19, 2026. **Decision: result-only capture on the tested Codex desktop
execution path.** The live OTLP adapter, receiver, API, Tool calls dashboard and
finite replay are implemented. This does not establish coverage for every client,
plugin, direct call, or subagent. See [setup and rollback](PLUGIN-TRACING.md).

## Installed-client evidence

| Check | Result |
| --- | --- |
| Desktop application | ChatGPT/Codex Desktop 26.1111.21308, build 27194 |
| Bundled engine | `codex-cli 0.169.0-alpha.11.1` |
| Existing app-server | Default stdio; no named attachable observer socket found |
| Export configuration | User-level `[otel]`, binary OTLP HTTP to an authenticated loopback receiver |
| Configuration validation | Installed engine accepted strict startup-config validation in an isolated home |
| Restart | User restarted the app after the probe and configuration were ready |
| Known operations | Harmless Railway `search_docs` lookups; no account/balance operations |
| Nested sequential calls | Separate attributed results observed |
| Nested parallel calls | Separate attributed results observed |
| Production acceptance | Real Railway calls reached the Rust receiver, query API and browser |
| Payload retention | None: only allowlisted metadata retained; no raw batches written |
| Rollback | Removed the exact temporary block while preserving concurrent configuration changes |

The initial November 18 passive-log check found no call events after a successful
Railway lookup. That observation did not rule out an active exporter. The first
restarted probe accepted 20 batches and 60 records, including 17 tool results.
Its original fixed tool-name vocabulary could not attribute those results.

A bounded metadata inspection then established that the client separates the
namespace from the tool name. A fresh namespace-aware probe observed four Railway
results for a known sequence of two sequential and two parallel nested calls.
The Rust collector subsequently verified the same source shape on real calls;
its first production check returned two Railway rows with durations 1,185 ms and
567 ms. These were separate calls, not revisions of a wrapper.

Configuration was initially restored after the finite capture, preserving other
edits. The user subsequently requested persistent tracing; the follow-up below
supersedes that temporary configuration state. No additional app restart was
forced and no app-server impersonated observation of the existing app.

## Observed source contract

| Source field | Interpretation |
| --- | --- |
| `event.name = codex.tool_result` | Semantic result event; authoritative over the separate OTLP callsite/event-name field |
| `tool_namespace = mcp__codex_apps__railway` | Exact namespace verified by known Railway operations |
| `tool_name = _search_docs` | Source tool component, kept separate from namespace during decoding |
| `mcp_server = codex_apps` | Transport server, not a plugin identifier |
| `conversation.id`, `call_id` | Correlation inputs transformed into collector-local opaque labels |
| `duration_ms` | May be a decimal string; bounded numeric duration, not arrival-time latency |
| `success` | May be a boolean string; reported execution success only |
| OTLP event timestamp | Used when present; otherwise completion time remains unavailable and receipt time is labeled |

The first production decoder rejected batches because it incorrectly required the
OTLP event-name field and Codex's `event.name` attribute to match. The source
contract now gives the semantic attribute precedence, with a regression fixture.
The production endpoint then accepted real Railway batches without that rejection.

The original adapter mapped only the exact Railway namespace. The follow-up
adapter accepts every hosted namespace suffix as an observed provider label,
without an allowlist or splitting internal underscores. Other namespaces remain
qualified tool names with no asserted manifest identity. No script parsing or
fabricated child traces are used. Built-in wrappers and local commands also
appear, as separate records. A global client exporter can cover multiple chats;
execution labels keep their call identities separate.

Only completed results are adapted. `codex.tool_decision` is not treated as a start.
A controlled local shell exit of 7 still produced source execution success, so
`success` is explicitly not treated as a provider/business outcome. Tool `isError`
is not inferred from text or result bodies. Cancellation, denial, parent links,
subagents, and direct model-call coverage remain unverified. No subagent was
spawned for this audit.

## Adapter and transport

`src/tool_trace/otlp.rs` implements the tested OTLP logs/protobuf subset, with fixed
wire traversal limits and no allocation of unknown payload fields. It uses the
upstream [OTLP log schema](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/logs/v1/logs.proto)
and the Codex [tool-result emitter](https://github.com/openai/codex/blob/main/codex-rs/otel/src/tool_result.rs)
to interpret metadata. Installed-client observations, not upstream source alone,
establish compatibility.

The receiver has a separate loopback listener, a private generated credential,
strict Host/Origin/content checks, a 1 MiB request cap, four connections, two-second
total deadlines, and a four-batch / 1,024-observation queue. Rejections and ignored
non-tool telemetry are visible. HTTP 200 means accepted into an in-memory pipeline,
not durable storage. Saturation returns 503 and clients can retry. Retry results
with the same execution/call identity do not create additional calls.

The correlator, bounded store, frozen-watermark pagination, opaque filter-bound
cursors and copy/inspector redaction are shared by live intake and offline replay.
Browser requests inherit the existing dashboard's read-only loopback protection.
SSE carries coalesced status/watermarks rather than an unbounded raw event log.

## Reproducing the probe

The finite probe remains available for future client versions. Use Python 3.11+
for its test suite. It never edits client configuration and creates private setup
fragments without overwriting files.

```sh
python3 -m unittest discover -s scripts -p 'test_trace_probe*.py'
probe_dir=$(mktemp -d)
python3 scripts/trace_probe.py --port 4318 --seconds 600 \
  --inspect-tool-names --config-out "$probe_dir/otel.toml" \
  --output "$probe_dir/metadata.json"
```

Default reports use fixed event/tool vocabulary, field-presence indicators and
per-run keyed ID/tool-name digests. Explicit tool-name inspection additionally
retains tool/namespace/server labels capped at 128 ASCII label characters. It does
not retain arguments, outputs, prompts, credentials, or raw correlation IDs.
The probe is finite and single-request-at-a-time, not the production receiver.

## Validation

- Eleven Python probe tests; twenty-one Rust trace-core/adapter/receiver tests.
- Actual-binary CLI replay and HTTP security/query integration tests.
- Browser acceptance covers live intake, duplicate delivery, frozen outcomes and
  details, filters, unknown attribution, pagination, clipboard redaction, narrow
  widths, and offline web replay. Existing MCP dashboard behavior is retained.
- Default, no-default-features, and snapshot-notifications-only Rust tests and
  Clippy configurations are checked. The receiver is explicitly feature-gated;
  finite replay and passive modes remain available without networking.
- The finite load run processed 6,000 result-only calls at 100/second, then a
  10,000-call burst. It retained at most 2,000 calls, handled nine backpressure
  retries, measured about 13.5 MiB peak RSS and 12 ms receipt-to-query p95, and
  stopped in about 238 ms. See the [benchmark record](validation/plugin-trace-benchmark-2026-11-19.json).
  Browser render latency and exporter buffering are excluded from that metric.
- Package source inventory includes the adapter, receiver, web assets, docs and
  synthetic fixtures. Nothing has been published or deployed by this work.
- Both 24-hour soaks remain optional and non-blocking; neither was run here.

The [compatibility record](validation/plugin-trace-capture-2026-11-19.json) contains
only metadata. The [November 18 record](validation/plugin-trace-capture-2026-11-18.json)
is retained as historical evidence of the earlier pending capture decision.

## Persistent multi-provider follow-up

At the user's request, installed a per-user macOS LaunchAgent with a private,
independent binary and stable credential. Reused the already-loaded endpoint/key,
so current capture continued without an additional app restart; the user config
now retains the exporter for future restarts. Verified enable, repeated enable,
disable preserving unrelated config, re-enable, and automatic launchd recovery
after forced collector termination. No logout/reboot was forced.

After recovery, a parallel GitHub `get_user_login` and Railway `search_docs`
produced separate real rows: GitHub 406 ms, Railway 1,182 ms, both reporting
execution success. Provider response contents and identity values were not
retained in this record. The browser displayed both rows and the API supplied
`github` and `railway` as automatic filter suggestions. This verifies these
providers on the tested execution path, not every installed plugin or subagent.

The native macOS arm64 plugin preview includes its executable and an onboarding
skill. Verified packaged paths, executable permissions, checksum, dependency
notices, exclusion of local credentials, and standalone `tracing status` from an
extracted binary. Desktop marketplace installation itself remains untested; the
installed-plugin CLI timed out. No plugin catalog is required for capture.

See the [persistence validation record](validation/plugin-trace-persistence-2026-11-19.json)
for the follow-up checks and remaining limits.

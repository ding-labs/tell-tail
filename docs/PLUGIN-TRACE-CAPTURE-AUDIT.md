# Plugin trace capture audit

November 18, 2026. **Capture decision pending. Live desktop tracing is not yet
verified or implemented.** The source-independent trace core and finite replay
are implemented; they are not evidence of desktop plugin coverage.

## Observed environment and checks

| Check | Result |
| --- | --- |
| Installed desktop application | ChatGPT/Codex Desktop 26.1111.21308, build 27194 |
| Bundled execution engine | `codex-cli 0.169.0-alpha.11.1` |
| App-server transport inspected | Existing processes use default stdio or explicit `stdio://` |
| Attachable control socket | None found on the inspected app-server processes; no private IPC interception attempted |
| User configuration | No `otel` section or user hook definitions found; managed/effective policy remains unverified |
| Candidate OTLP configuration | Accepted with `--strict-config --print-startup-config` in an isolated temporary `CODEX_HOME` |
| Existing configuration modified / client restarted | No / no |
| Real plugin operation | One Railway `search_docs` lookup for public healthcheck documentation succeeded |
| Passive log tail after that operation | 1,827 appended bytes, six physical lines; no Railway, `codex.tool_result`, call-ID field, or MCP tool-call marker matched |
| Active telemetry reception from this app | Not tested: no exporter has been enabled on its running execution path |
| Direct, nested, parallel, cancelled, subagent call coverage | Unverified |

The log-tail check is a bounded observation, not proof that the app lacks a tracing
mechanism. Configuration acceptance proves syntax, not export behavior, policy
permission, source completeness, or plugin attribution. No standalone app-server
was launched to impersonate observation of this desktop app.

The installed app-server schema was generated and checked for supported methods.
Its presence alone does not establish an observer connection to the running app.
Reading stored chat history or resuming threads is not a substitute for an
execution-event stream in this feature.

The upstream [telemetry](https://learn.chatgpt.com/docs/config-file/config-advanced#observability-and-telemetry),
[app-server](https://learn.chatgpt.com/docs/app-server), and
[hook](https://learn.chatgpt.com/docs/hooks) documentation was rechecked. Local
configuration and local hooks must not be assumed to cover a cloud-orchestrated
chat just because some tools execute on this computer.

## Implemented feasibility probe

`scripts/trace_probe.py` implements the finite probe described by milestone 0a.
It accepts OTLP HTTP/protobuf on loopback, generates a receiver credential in a
new mode-0600 configuration fragment, and never edits client configuration.

The probe has a 1 MiB body cap, 1,024 records per batch, 4,096 retained summaries,
bounded field traversal, a two-second total connection deadline, and explicit
duration/request limits. It rejects browser origins, incorrect Host/credentials,
unsupported encodings, oversize inputs, malformed protobuf, and ambiguous
duplicate attributes. It is single-request-at-a-time, not a production collector.

Only a fixed event/tool vocabulary, field-presence indicators, validated numeric
duration/boolean outcome, and per-run keyed ID digests are retained. Resource
attributes, arbitrary bodies, outputs, prompts, credentials, and raw identifiers
are not copied into reports. "Unidentified" is intentional: probe classification
must not guess arbitrary plugin names. Execution success is not a verified
provider-level business outcome.

Protobuf field numbers follow the upstream
[OTLP log schema](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/logs/v1/logs.proto).
The restricted decoder is not a general OTLP implementation. Unsupported inputs
receive rejection rather than being reinterpreted as custom JSON.

```sh
python3 -m unittest discover -s scripts -p 'test_trace_probe*.py'
```

## Next live experiment

The outstanding restart timing question is user-facing because restarting this
app interrupts active chats. Global settings have not changed while that question
is pending. A restart alone is insufficient: the exporter must first be explicitly
configured to the running probe.

1. Choose a window to restart the relevant client process and keep the probe
   running outside the app being restarted, in a separate terminal.
2. Create a fresh private configuration fragment and metadata-report destination:

   ```sh
   trace_probe_dir=$(mktemp -d)
   python3 scripts/trace_probe.py --port 4318 --seconds 600 \
     --config-out "$trace_probe_dir/otel-fragment.toml" \
     --output "$trace_probe_dir/metadata.json"
   ```

3. Review and merge the generated `[otel]` fragment into the configuration that
   actually controls this execution path. It contains a local credential; do not
   publish it or put it in this repository. Preserve any existing exporter and
   applicable policy. For cloud orchestration, first establish whether a local
   exporter applies; do not assume remote traffic can reach localhost.
4. Restart the relevant app/process, invoke one known Railway documentation tool,
   and inspect the sanitized report. Verify tool identity and correlation fields,
   not merely arrival of unrelated telemetry.
5. Repeat with two sequential nested calls, parallel calls, and a controlled local
   failure/cancellation. Test subagent coverage only where the source supports it.
6. Restore the prior tracing configuration and restart as required. Remove the
   private fragment after the probe is no longer needed. Do not leave the client
   exporting to a stopped temporary receiver.
7. Record the execution paths and fields actually observed. Select the production
   adapter only after the evidence passes the plan's checkpoint.

If events expose only `functions.exec`, record wrapper-only coverage. Do not turn
script text into invented child traces. If starts are absent, record result-only
coverage. If no usable source is exposed, record the missing upstream capability
instead of advertising complete tracing.

## Source-independent implementation available now

The Rust core implements an allowlisted normalized observation contract, local
opaque IDs, lifecycle correlation, source-scoped duplicate delivery handling,
bounded call/revision storage, frozen-watermark queries, filter-bound cursors,
and explicit incomplete/conflict states. Old snapshot queries expire explicitly
when retention can no longer reconstruct them.

The finite replay command is usable without reading desktop logs or requiring
the `web` feature:

```sh
cargo run --locked -- mcp \
  --trace-replay tests/fixtures/tool-trace/lifecycle.ndjson \
  --output json --once
```

Replay outputs versioned `call_update`, `trace_gap`, and `trace_source` NDJSON.
IDs, revisions, and counters are decimal strings. Updates sharing a call ID are
revisions of one call, not additional invocations. End-of-input marks unmatched
starts incomplete and labels the source `synthetic_replay`; it never claims live
capture. Input is limited to the regular file's initial size, with bounded line
framing. Invalid records produce safe gap notices without echoing payloads.

Replay currently supports text and JSON only. There is no production trace
listener, live adapter, trace web API, or Tool calls dashboard tab yet. Existing
MCP dashboard and passive collection behavior is unchanged. See the
[implementation plan](PLAN-PLUGIN-TRACING.md) for the remaining milestones.

## Validation of this increment

- Eight probe tests passed, covering metadata suppression, protobuf limits,
  duplicate/conflicting fields, HTTP access/encoding checks, retention saturation,
  and private configuration-file creation.
- Fourteen trace-core unit tests and three trace CLI integration tests passed.
  They cover scoped IDs, late starts, source duration, clock reversal, terminal
  conflicts, gaps, duplicate delivery, missing IDs, frozen queries, pagination,
  exact counters, malformed/oversize input, and bounded retention under 2,500 calls.
- The full Rust test suite and all-target Clippy with warnings denied passed
  sequentially for default, no-default-features, and snapshot-notifications-only
  builds. The default binary was rebuilt afterward.
- No trace browser UI or production adapter exists to validate yet. Live capture,
  end-to-end benchmarks, and packaged feature acceptance remain pending.
- Neither 24-hour soak was run; both remain optional and non-blocking.

# HTTP status monitoring implementation plan

Status: proposed work after 0.3.0. The flags, output extensions, and HTTP metrics
in this plan do not exist in 0.3.0. The current parser counts a standard combined
access-log record as unknown, including a record whose response status is 500.

## Outcome and design decisions

Make Nginx/Apache access logs and structured application access logs useful with
one format or field selector. Show response classes, exact status counts, recent
5xx examples, and alerts whose denominator is HTTP response records. Preserve
application log severity as a separate fact when the producer supplies it.

A status-bearing log record is an observation, not a unique request identity.
Several proxies can log one request, an application can emit multiple records,
and logging can be sampled or filtered. Name rates and percentages accordingly;
do not claim deduplicated request throughput, service availability, latency, or
an SLO from these inputs.

Decisions for the first complete HTTP slice:

- HTTP interpretation is explicit. `--format access` enables it for conventional
  Common/Combined access logs; `--http` enables structured field extraction.
  Existing commands retain their current classification and alert behavior.
- Extract response status from a defined field position/path, never from a
  three-digit number found anywhere in a message, URL, byte count, or trace ID.
- Count status independently of severity. A valid explicit `level=info` with a
  response status of 500 contributes to info severity and HTTP 5xx.
- Derive severity from status only when the selected source has no severity
  value, unless the user explicitly chooses another HTTP severity policy.
- Store fixed-size response-class windows and exact-code session totals. Do not
  allocate a hash map keyed by paths, clients, hosts, request IDs, or user agents.
- Add an HTTP-specific alert with an explicit HTTP denominator. Preserve the
  existing all-log-record error rule and its wire payload when HTTP is disabled.

## Proposed commands and boundaries

```sh
# Nginx's standard combined log, or Apache Common/Combined
tell-tail /var/log/nginx/access.log --format access --output web --open
tell-tail /var/log/apache2/access.log --format access

# Conventional structured status fields, with explicit HTTP opt-in
my-server 2>&1 | tell-tail --http --output web --open

# Literal JSON/logfmt key or JSON Pointer to a nested value
tell-tail access.ndjson --http-status-field status
node server.js 2>&1 | tell-tail --profile pino --http-status-path /res/statusCode

# Keep source severity only while still collecting HTTP status metrics
tell-tail access.ndjson --http --http-severity off

# HTTP 5xx threshold with an HTTP-response denominator
tell-tail access.log --format access --alert-http-status 500-599 \
  --alert-http-percent 5 --alert-webhook "$ALERT_WEBHOOK_URL"
```

All names above are proposed CLI contracts to implement and test. Fields/paths
imply HTTP mode. `--http-status-field` and `--http-status-path` are mutually
exclusive, share the existing key/path bounds where applicable, and are separate
from severity selectors. A Pino/Python/OTel/Serilog severity profile may coexist
with HTTP field selection in JSON; it does not itself imply an HTTP record.

`--format access` conflicts with severity profiles, severity field/path selectors,
and HTTP field/path selectors because it has a fixed positional grammar. Saved
Docker/CRI envelopes may wrap an access record under the existing one-layer
contract. Unsupported partial or multiline envelopes remain unknown and produce
no HTTP observation. Apple and journal formats reject HTTP options initially;
parsing HTTP records inside a native-source `MESSAGE` is separate composition
work, not an implicit interpretation change.

`--http` is allowed only with auto, JSON, or logfmt input. In auto mode, ordinary
text still counts normally but supplies no HTTP status. Explicit HTTP fields
never turn the number `500` in arbitrary console text into a status.

## Formats and extraction

### Common and Combined access logs

Add a bounded scanner for these exact layouts:

```text
remote-host ident auth-user [timestamp] "request" status bytes
remote-host ident auth-user [timestamp] "request" status bytes "referer" "user-agent"
```

Nginx's default is Combined; its format is configurable, and escaping can differ.
Apache's `%>s` is the final response status, whereas `%s` can describe the original
request before an internal redirect. These distinctions define the initial
supported fixtures and documentation examples.
[Nginx logging](https://nginx.org/en/docs/http/ngx_http_log_module.html)
· [Apache logging](https://httpd.apache.org/docs/2.4/mod/mod_log_config.html)

Consume the complete record, accepting LF/CRLF framing and horizontal whitespace
at field boundaries. Scan quoted fields with explicit escape handling so quotes
inside user agents or requests cannot shift the status position. Validate the
fixed grammar and the numeric byte field (or `-`); do not store or use response
bytes as Tell-Tail's input-byte count. The host may be IPv4, IPv6, or a hostname;
ident and auth-user may be `-`. Treat the request as opaque quoted content, so
`-`, malformed requests, CONNECT, OPTIONS `*`, HTTP/1.0, HTTP/1.1, HTTP/2.0, and
HTTP/3.0 labels do not move the status column or require URL parsing.

Accept a documented union of the producer escapes needed by the canonical
fixtures: escaped quote/backslash, common control-letter escapes, and `\xHH`.
Do not decode whole fields merely to locate status. Reject incomplete escapes,
unclosed quotes/brackets, invalid UTF-8, missing required fields, and unexplained
trailing columns. Timestamp validation is structural, never event-time metric
assignment. Comments and blank lines do not become successful requests.

A structurally complete access record with a missing status marker `-`, `000`, or
another unsupported status value is an HTTP candidate with unavailable/invalid
status. It still counts once as a log record. A broken record increments the
ordinary parse-failure counter once; do not double-count malformed status and
malformed line as two input records.

Initial support intentionally excludes arbitrary custom Nginx `log_format` or
Apache `LogFormat` templates. Extra vhost prefixes, appended timings, JSON-inside-
quoted-fields, proxy formats, and ingress-controller variants need explicit
named presets backed by real fixtures or structured JSON. A broad greedy regex
that happens to find a status is not format support. List tested layouts in the
user guide so unsupported logs produce actionable guidance.

### Structured JSON and logfmt

With `--http`, inspect a small documented allowlist of literal top-level keys:
`status`, `statusCode`, `status_code`, `http.status_code`, and
`http.response.status_code`. Do not recursively search every object. Nested
framework shapes use `--http-status-path`; literal dotted keys remain distinct
from nested paths. These keys are only interpreted after HTTP opt-in.

An explicit selector disables inferred field selection. When several automatic
candidates are present, all must normalize to the same status; disagreement or
one unsupported value makes HTTP status unavailable with a conflicting-field
hint. Duplicate selected keys are malformed, even when equal. A valid log
severity can still be retained when only HTTP field values conflict; malformed
JSON retains the existing whole-record failure rule.

Accept a JSON integer from 100 through 599 or a string of exactly three ASCII
digits in that range. Accept those strings in logfmt, quoted or unquoted.
Reject exponent/fraction forms, signs, spaces inside the value, arrays, objects,
booleans, and lists such as `"502, 200"`. Null/missing, unsupported values, and
conflicting fields are distinct diagnostic states. Numeric strings are an
intentional HTTP-specific rule; do not broaden the existing Pino or Python
numeric severity conventions.

Use streaming visitors and the existing JSON Pointer limits: at most 16 object
keys and 1024 UTF-8 bytes, with `~0`/`~1` escapes and no array indexing. Validate
ignored JSON and Unicode as today. Keep an explicit distinction between absent,
present-invalid, present-unsupported, and recognized severity so HTTP fallback
does not erase a bad source level.

Add fixture-backed recipes for Nginx JSON `$status`, Apache JSON with final
`%>s`, Pino HTTP's selected response field, and standard OTel response-status
attributes. A flattened OTel JSON log record is different from an OTLP batch;
this feature does not ingest OTLP, spans, or binary protobuf.
Use the [Pino HTTP serializer contract](https://github.com/pinojs/pino-http)
and [OTel HTTP attributes](https://opentelemetry.io/docs/specs/semconv/registry/attributes/http/)
as fixture references. Retain the older `http.status_code` key as compatibility
input while documenting `http.response.status_code` as the current OTel name.

### Proxy and nonstandard values

The selected status describes one explicit role: the final response recorded by
that field. Do not use an upstream-attempt field as a fallback for final status.
Nginx upstream status can contain several attempt outcomes; retain it as an
unsupported value unless a separate upstream-attempt feature is designed.
[Nginx upstream variables](https://nginx.org/en/docs/http/ngx_http_upstream_module.html#var_upstream_status)

Accept all numeric codes 100–599, including unregistered/vendor codes, and group
by first digit. Keep exact numeric identity; a small label table may explain
common codes but must not reject other valid-range values or invent meanings.
Values such as 444, 499, 520, and 599 remain distinct; identify vendor semantics
only in a producer-specific guide. Values 000, 099, 600, and 999 are invalid HTTP
statuses for this monitor, not automatically 5xx.

HTTP defines the 100–599 range and the five response classes; it also distinguishes
interim from final responses. The monitor is reading logs, not implementing a
client's recovery behavior for an invalid wire response.
[HTTP semantics, section 15](https://www.rfc-editor.org/rfc/rfc9110.html#section-15)

A logged 200 with a failed gRPC status is still HTTP 200. Application outcomes,
WebSocket closure codes, TLS errors with no response, and proxy transport error
codes need separate fields/metrics. Do not label them HTTP 500 by inference.

## Severity policy

Proposed `--http-severity` values are `fallback` (default in HTTP mode), `off`, and
`prefer-status`. The mapping is a Tell-Tail display/alert policy, not a protocol
statement that every 4xx is an application warning.

| Status class | Derived severity | HTTP class |
| --- | --- | --- |
| 1xx | info | informational |
| 2xx | info | success |
| 3xx | info | redirection |
| 4xx | warn | client error |
| 5xx | error | server error |
| Missing/invalid/conflicting | no derived severity | no recognized status |

No HTTP status implies fatal, and 3xx never implies warning merely because it is
not 2xx. Preserve distinctions such as 304, expected 404s, authentication 401/403,
rate-limiting 429, and gateway 502/503/504 in exact-code views. The first release
uses the fixed mapping; custom maps require separate bounded configuration.

| Source severity | Status | Fallback result | HTTP result |
| --- | --- | --- | --- |
| absent | 500 | error | 5xx / 500 |
| info | 500 | info | 5xx / 500 |
| error | 200 | error | 2xx / 200 |
| unsupported or conflicting | 500 | unknown, with source hint | 5xx / 500 |
| absent | 404 | warn | 4xx / 404 |
| absent | 000 | unknown | invalid status |
| malformed JSON/access grammar | apparent 500 | unknown + parse failure | no recognized status |

`off` never derives severity. `prefer-status` uses valid HTTP status even when a
source level exists; when status is unavailable it preserves normal severity.
Explain that enabling it changes the existing log-error threshold behavior.
This override must be explicit and visible in the source/parsing disclosure.

## Counting, windows, and memory bounds

Carry one optional HTTP assessment alongside ordinary classification. A log
record increments the general record count once. HTTP metrics describe a subset
or an independent property of those records; they are not extra log records.
Keep the current physical framing, outer input-byte accounting, 100 ms rolling
window, monotonic processing time, and completed-second history alignment.

Maintain these bounded counters:

- Session counts for each exact code 100–599: a fixed array of 500 u64 values.
- Session and rolling counts for each of the five response classes.
- HTTP candidate records, recognized status records, and mutually exclusive
  unavailable categories: missing, unsupported/invalid, conflicting, malformed,
  and oversized when the format identifies the record as an HTTP candidate.
- Completed-second class histories aligned with the existing 60-second chart.
- One fixed rolling numerator for the configured HTTP alert's selected code set.

A candidate is every record in explicit access format, or a structured record
with a selected HTTP field present. With an explicit field/path selector, every
structurally matching JSON/logfmt record is a candidate even when that value is
missing. Ordinary text and unrelated JSON in allowlist mode remain non-candidates.
Malformed/oversized records whose type is unknowable in mixed auto mode remain
ordinary diagnostics; do not invent an HTTP denominator for them.

HTTP status coverage is recognized/candidate, not recognized/all logs. Empty
coverage is null. Display its denominator so an all-log severity coverage value
is not mistaken for HTTP coverage.

Define the default final-response denominator as recognized 200–599 records.
Report 1xx separately; 101 upgrades are visible separately and excluded under
this initial denominator policy. This avoids counting interim log entries as
additional completed requests. Do not deduplicate records or infer whether a
later final response exists. Label `5xx / observed final-response records`,
not simply `error rate`. Unknown statuses do not count as successes; show
coverage beside the ratio and in alert payloads.

For example, 90 records with 200, 10 with 500, 10 missing selected status, and
5 with 103 yield 115 HTTP candidates, 105 recognized statuses, 100 denominator
records, and a 10% 5xx fraction. Existing log-error ratios retain their all-log
record denominator and selected severity policy.

Do not store 500 exact-code counters in each 100 ms bucket: at a 3600-second
window that alone would exceed 137 MiB. Store only classes, bounded diagnostics,
and one selected-alert numerator per bucket. Exact-code breakdown is session-only
in the first release and must be labeled as such. If rolling exact-code views
are later requested, use a separately bounded selected-code set and disclose its
memory cost and time resolution before implementation.

Allocate HTTP state only when enabled. Budget <=8 MiB incremental monitor state
at the maximum 3600-second window, with a documented bucket-size calculation and
measured RSS. Preserve the existing total resource gates. Snapshot transfer must
not copy all 500 values on every ingest tick: use a fixed published aggregate
with bounded/coalesced copies and measure the cost.

## Displays, outputs, and compatibility

The browser and TUI should show HTTP mode only when configured. Add class counts,
rolling 4xx/5xx fractions, coverage, a 60-second class chart, and a clearly labeled
session code breakdown. On small terminals, prioritize 5xx, denominator, and
coverage rather than squeezing a table into the existing layout. Freeze applies
to all HTTP numbers, histories, and samples; connection/failure status stays live.

Show recent 5xx examples even when explicit application severity is info. Extend
the existing single 20-entry, 512-byte preview ring to retain records that are
error/fatal OR HTTP 5xx; a record matching both is inserted once. Preserve source
severity and attach an optional status. Add separate `errors`, `fatal`, and
`HTTP 5xx` filters, with a precise default label. Do not add a second unbounded
sample store or persist request data. Existing sanitization and clipping apply.
4xx totals and codes are available without filling the ring with routine 404s.

Plan the wire contracts before adding fields to structs:

| Surface | Proposed contract |
| --- | --- |
| CLI JSON without HTTP | Existing schema 2 and golden fixtures unchanged |
| CLI JSON with explicit HTTP mode | Schema 3; all existing fields plus a named `http` object; document opt-in and counters/ratios |
| Web frames | Optional named `http` object under schema 1 if genuinely additive; old frames omit it, new parser validates it |
| Parser evidence | Keep the six-position array stable; named HTTP extension separates extraction from severity attribution |
| Text | Explicit HTTP block only when enabled; denominator and session/rolling labels included |
| Prometheus | New opt-in HTTP families; old names and semantics unchanged |
| Existing generic alerts | Schema 1 and event names unchanged |
| HTTP generic alerts | Schema 2 with a distinct rule kind, selected code ranges, numerator, denominator, coverage, and threshold |

Do not count one record as both a new HTTP severity parser and an old JSON
severity parser in the recognized-rule sum. Extraction evidence is separate from
which rule supplied final severity. Invalid HTTP fields must not invalidate an
otherwise sound source-severity field unless the whole input syntax is invalid.

Use exact decimal u64 strings in web frames as today. A code breakdown is a
bounded sorted array of code/count pairs, with at most 500 entries. Keep the
64 KiB web frame ceiling, checking worst-case 20-digit counts, all codes,
60-second histories, parser diagnostics, and full previews. Adjust the proposed
payload before release if it cannot fit; do not silently truncate counts or
raise the limit without reviewing resource bounds and browser validators.

Proposed Prometheus families include `tell_tail_http_responses_total{status}`, a
rolling class-count gauge with five fixed class labels, HTTP candidate/recognized
counters, and ratio gauges with documented undefined behavior at zero records.
Emit stable zero-valued exact-code series for all 500 codes when HTTP is enabled,
so missing series do not change meaning between scrapes. Measure file size and
serialization cost. Never attach route, URL, user, or request labels. Keep the
atomic textfile writer and its stale/running signals; this work does not add an
HTTP scrape endpoint.

## HTTP alerts

Introduce `--alert-http-status` for a bounded code-set expression such as
`500-599`, `429`, or `429,500-599`, plus `--alert-http-percent`. Parse at startup
into a fixed 500-bit membership set; accept at most 32 canonical items, reject
reversed/out-of-range ranges, and normalize overlap. Initial alert selection is
limited to 200–599, matching the defined final-response denominator.

Both flags require the existing endpoint. The first version allows either the
existing log-error rule or one HTTP rule in a process, with conflicting CLI
options rejected. This preserves one rule latch and the existing one-in-flight
plus one-queued delivery bound. Multiple simultaneous rules are a separate change
requiring fair scheduling and independent cooldown state; do not accidentally
let the first rule starve the second.

For HTTP rules, minimum samples means final-response records in the window;
reuse `--alert-min-records` with mode-specific help. Add an explicit optional
minimum HTTP coverage threshold if noisy streams need it, but select its default
and behavior before implementation. The proposed default is no coverage gate,
with recognized/candidate counts included in every HTTP alert. Unknown status
records remain visible rather than silently lowering the response-error ratio.

Retain inclusive breach thresholds, strict-below recovery, eligible minimum
samples, recovery bypassing cooldown, and state committing on queue acceptance.
Empty traffic, source failure, and shutdown are not recovery. Delivery remains
best effort with no durable retry. Slack/Discord messages must say `HTTP 5xx`
or the selected code set and include denominator/coverage, rather than reuse
`errors + fatal` wording. Existing alert tests remain unchanged; new tests cover
the independent HTTP arithmetic and payloads.

## Code owners and data flow

Introduce a copyable `HttpAssessment` containing a bounded status/presence result
alongside the existing classification assessment. Keep syntax parsing, severity
policy, statistics, and rendering in separate owners so tests can isolate each
contract. Preserve existing public classifier entry points with wrappers where
possible; document any unavoidable Rust library API changes in the release.

| Owner | Responsibility |
| --- | --- |
| `src/cli.rs` | HTTP options, selectors, policy, alert expression, and conflict checks |
| New `src/parse/access.rs` and HTTP helper module | Access scanner, status normalization, field assessment |
| `src/parse.rs`, `src/parse/logfmt.rs` | Streaming field extraction and presence/precedence |
| `src/engine.rs`, `src/stats.rs` | Single-record accounting, optional HTTP state, class windows/history |
| `src/recent.rs` | One bounded ring with severity and HTTP eligibility/filter metadata |
| `src/output.rs`, `src/prometheus.rs`, `src/web_frame.rs` | Explicit serialization contracts and bounded payloads |
| `src/tui.rs`, `web/frame.mjs`, `web/render.mjs`, `web/dashboard.mjs` | Validation, rendering, review filters, and frozen snapshots |
| `src/alerts.rs`, `src/slack.rs`, `src/discord.rs` | HTTP arithmetic and distinct payloads with existing delivery machinery |
| `src/input/demo.rs`, `tests/`, `fuzz/`, `browser-tests/` | Deterministic examples, independent oracles, regressions, and performance evidence |

## Implementation sequence

| Step | Focused increment | Acceptance |
| --- | --- | --- |
| H1 | HTTP assessment model, presence states, code normalization, CLI conflicts | Full 100–599 normalization table; invalid and missing cases; existing classification unchanged |
| H2 | Canonical Common/Combined scanner | Apache/Nginx fixtures, escaping, adversarial columns, negative corpus, and chunking pass |
| H3 | Structured JSON/logfmt selectors and precedence | Literal/nested fields, mixed severity/status, duplicates, conflicts, and profiles pass |
| H4 | Severity fallback and evidence | Table above holds; unsupported explicit levels stay unknown; no double-counted parser attribution |
| H5 | Bounded class windows, exact-code session totals, coverage | Independent timed reference model; maximum-window memory budget; zero/idle/rollover invariants |
| H6 | CLI JSON 3, text, Prometheus, and web frame contracts | Versioned fixtures, schema validation, exact large counters, Promtool, worst-case frame size |
| H7 | Browser/TUI views and recent HTTP 5xx review | Freeze/resume, filters, compact layout, accessibility, reconnect, and EOF states |
| H8 | HTTP alert rule and Slack/Discord formatting | Correct denominator and selected codes; breach/recovery/cooldown/backpressure behavior |
| H9 | Installed demonstrations and documentation | Reproducible Nginx/Apache/JSON examples from packaged Mac/Linux binaries |

Commit each increment and its necessary tests after verification. H2/H3 should
share normalization and assessment, not duplicate policy. H6 precedes H7 so UI
fixtures describe a stable contract. Do not ship an HTTP headline that only
changes severity while omitting HTTP denominators and coverage.

## Verification matrix

Test every integer 100–599 and boundary values 0/99/100/199/200/299/300/399/400/
499/500/599/600/999; exact strings, floats, exponents, nulls, wrong types, leading
zeros, signs, extra digits, and invalid Unicode. Include unregistered codes and
protocol labels from HTTP/1.x, HTTP/2, and HTTP/3 without interpreting message
body numbers as status.

Fixtures must cover Common/Combined, IPv6, absent user/ident, escaped quotes and
backslashes, Nginx hex escapes, Apache final-status examples, empty/malformed
requests, byte counts equal to 500, URLs containing `/500`, request/user-agent
text containing status-like tokens, referer `-`, tab separators, trailing custom
columns, JSON/logfmt keys, nested paths, conflicting automatic candidates,
source severity disagreeing with HTTP status, and upstream attempt lists.
Add negative examples from ordinary syslog, Python console, and non-HTTP JSON.

Generate canonical records with an independent expected-status oracle. Verify
all framing chunk sizes, oversized input, finite unterminated EOF, rotation, and
partial container chains. Fuzz scanner/JSON extraction and the timed HTTP ledger;
include maximum quoted fields and adversarial escaping to catch quadratic scans.

Use independent counts for session classes, code totals, window expiry, coverage,
alert numerator/denominator, and completed-second histories. Check that code
counts sum to recognized statuses, class sums match code counts, and selected
alert counts never exceed the final-response denominator. Include mixed sources,
long idle gaps, clock-boundary arrival, empty startup, and window 1/60/3600.

Exercise all outputs and alert modes with valid 500 status plus INFO severity,
unknown severity plus 500, malformed records, and low coverage. Verify HTTP 5xx
previews are retained even with severity off and inserted only once when also
an error. Check controls remain sanitized in terminal, browser, and webhook text.

Run the existing default/CLI-only/notification Rust and clippy matrices, golden
compatibility fixtures, browser tests including real visibility, terminal
freshness, backpressure, and distribution checks. Compare paired throughput and
RSS for HTTP disabled, access scanning, structured HTTP, dashboards, maximum
window, all 500 codes, and worst-case frames. Re-run only the relevant changed
candidate validations; optional day-long soaks never become a release gate.

## Demonstration and rollout

Add a deterministic `--demo --http` scenario in a separate increment: steady
response volume, a 404/429 increase, then a 502/503 burst, silence, and recovery.
The demo labels synthetic data and exercises the ordinary parser/aggregation
paths. It should include INFO-level 500 records to show why response metrics and
application severity are distinct. Document flag conflicts with the existing
mixed-format and profile-generated demos before implementing the generator.

Publish `docs/HTTP-STATUS.md` with the actual supported layouts, producer config
examples, precedence table, ratio denominators, schema migration, upstream/gRPC
limits, and installed commands. Use final-response fields in Nginx/Apache
examples. Link a concise migration note: existing commands are unchanged; adding
HTTP mode can derive severity and affect the legacy error rule under the selected
policy. Validate both a clean install and an upgrade before announcing support.

## Coordination with Linux inputs

Both plans touch CLI validation, classification evidence, engine accounting, and
web frame validation. Land shared compatibility decisions before editing those
owners twice. Journal `PRIORITY` remains source severity; HTTP inside `MESSAGE`
is not parsed automatically. A future explicit journal-message envelope can
reuse HTTP extraction once its one-entry accounting and authority rules are
specified. Linux capture itself does not depend on the HTTP metrics feature.

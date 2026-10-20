# Local web transport decision

Follow-up: [direct snapshot notifications](SNAPSHOTS.md) now replace polling for
real inputs. The measurements below preserve the earlier transport experiment;
the follow-up records the new input-to-DOM results.

Decision: use SSE for the first local metrics dashboard. The October 20, 2026
local comparison found both transports comfortably responsive at the intended
10 Hz update rate. The differences do not justify WebSocket connection/reconnect
code for this one-way workload. This is a product choice supported by local
measurements, not a claim that SSE is universally the fastest protocol.

The [development-only experiment](../experiments/web-transport/README.md) is an
isolated Cargo workspace. It uses the actual Tell-Tail engine to capture a full
60-second history and sends identical JSON through both transports, encoded once
per publication. Its routes, frontend, dependencies, and lockfile are excluded
from the release package. The shipping CLI has not acquired a web output mode.

## Evidence and method

The [compact measurement record](validation/web-transport-2026-10-20.json) includes
host/toolchain identity, source commits, latency summaries, resource observations,
and real-input results. Full per-message reports remain locally under
`experiments/web-transport/results/` and can be regenerated with the commands below.

Host: Apple M3, eight logical CPUs, macOS 27.0 / Darwin 27.0.0. Browser:
Chromium 151.0.7922.34, Playwright 1.62.1, Node 24.15.0. Both the experiment and
the reused Tell-Tail library build on the pinned Rust 1.88.0 toolchain.

- Product workload: 60 runs at 10 Hz, with zero/one/four clients, representative
  and large-counter payloads, and five alternating SSE/WebSocket pairs per case.
  Each measured interval lasts five seconds after one second of warmup.
- Stress workload: 40 runs at 100 Hz, one/four clients, both payloads, and five
  alternating pairs per case. Each measured interval lasts three seconds after
  one second of warmup. This is synthetic publication stress, not an assertion
  that the engine produces 100 new snapshots/second.
- Across the two campaigns, browsers received 35,447 measured frames with no
  observed sequence gaps. DOM rendering deliberately coalesces updates.
- The same Chromium process is reused, with fresh server processes and browser
  contexts per run. Headless pages are held visible with background throttling
  disabled. CPU affinity, exclusive host use, and power state were not controlled.
- Browser/server monotonic clocks are mapped with the fastest of 12 local clock
  probes before measurement, then checked afterward. Timings start immediately
  before JSON encoding, after metrics conversion. They end at event receipt or
  DOM/SVG writes, not physical screen paint.

The largest initial clock uncertainty was 0.25 ms at 10 Hz; the largest before/after
offset change was about 0.31 ms. Sub-millisecond differences should not be ranked
as decisive wins. The artifact preserves clock estimates and tail outliers.

## Results at 10 Hz

Numbers below are medians of per-client/run p95 values, in milliseconds. They
are not percentiles pooled across all messages. Four-client rows contain 20
client/run observations per transport; one-client rows contain five.

| Payload / clients | SSE receipt | WebSocket receipt | SSE DOM | WebSocket DOM |
| --- | ---: | ---: | ---: | ---: |
| Representative / 1 | 0.87 | 0.86 | 1.65 | 1.64 |
| Representative / 4 | 3.86 | 3.88 | 4.81 | 4.71 |
| Large counters / 1 | 0.90 | 0.90 | 1.96 | 1.66 |
| Large counters / 4 | 4.61 | 4.07 | 5.46 | 4.61 |

The representative JSON frame is 1,368 bytes; the deliberately widened frame is
2,521 bytes. At 10 Hz that is approximately 13.4 or 24.6 KiB/second per client
before protocol/network overhead. Large counters include exact u64 decimal
strings and 60 maximum-width history points; their ledger is deliberately synthetic.

One large-counter/four-client SSE run had a synchronized tail spike across its
pages: approximately 85 ms to event receipt and 122 ms to DOM update. Its cause
was not isolated. Keep it in the evidence rather than discarding it or attributing
it to the protocol. The short runs are insufficient to characterize rare tails.

At 10 Hz, median replay-server CPU was approximately 0.2% of one logical core with
one client and 0.4% with four for either transport. These small values approach
`ps` accounting resolution; a reported zero in no-client controls does not mean
zero CPU work. Peak replay-server RSS across the campaign was 4,592 KiB.

## Stress and real input

At 100 Hz, the median per-client/run receipt p95 was 0.74–1.32 ms across the cases;
DOM p95 was 9.13–10.59 ms. The direction of the small transport differences varies
with the case. Roughly 60% of received frames were rendered, consistent with the
latest-value renderer at display cadence. This is intentional coalescing, not lost
metric counts. Replay-server CPU rose to roughly 1.3–2.0% of a logical core.
All replay-server shutdowns across both campaigns completed within 3.4 ms.

The separate input harness injected 25 marker trials per transport with idle
and nominally paced 20K-records/second input, through both stdin and a followed
file. It used one host monotonic clock from producer write to Playwright observing
the expected DOM count; automation cost was reported without subtraction.

| Source / load | SSE input-to-DOM p95 | WebSocket input-to-DOM p95 |
| --- | ---: | ---: |
| Stdin / idle | 218.7 ms | 191.1 ms |
| Stdin / paced | 205.8 ms | 183.7 ms |
| File / idle | 183.6 ms | 188.6 ms |
| File / paced | 190.3 ms | 189.1 ms |

All final counts matched the injected records exactly. Observation round-trip
p95 was 0.54–1.09 ms. Maximum per-case input-to-DOM p99 was 250.6 ms.
These are freshness/correctness checks, not paired protocol ranking or maximum
ingestion-capacity measurements: workload duration and publication phase vary.

The input harness currently samples `Running::snapshot()` at 10 Hz, which can
add another interval after the engine's approximately 100 ms publication step.
This explains a possible source of the much larger end-to-end delays; the
measurements do not isolate every component. Implement the planned coalescing
notification before deciding to increase update frequency or change transport.

## Correctness, dependencies, and remaining gates

Two experiment Rust tests and nine Node/browser/network checks pass, covering
engine-derived history, exact large counters, identical wire JSON, frame escaping,
measurement arithmetic, reconnect, freeze/resume, real stdin counts, stream
capacity/release, and stalled clients on both transports. The macOS stalled tests
confirm a pending kernel send queue, then verify continued publication, a second
client, and sub-second shutdown. A separate native-visibility test opens a temporary
desktop Chromium profile with Playwright's focus overrides disabled, switches
tabs, confirms stream release while hidden, and checks current-state resume.

The existing 52 Rust tests also pass. Experiment formatting and Clippy pass with
warnings denied. Source-package inspection confirms the experiment is excluded.

Selected locked backend dependencies:

| Package | Version | Declared minimum Rust | License |
| --- | --- | --- | --- |
| Axum | 0.8.9 | 1.80 | MIT |
| Tokio | 1.52.3 | 1.71 | MIT |
| tokio-tungstenite | 0.29.0 | 1.63 | MIT |
| tungstenite | 0.29.0 | 1.71 | MIT OR Apache-2.0 |
| Hyper | 1.12.0 | 1.63 | MIT |

This milestone does not establish the production under-5% ingestion-overhead
gate, a one-hour memory plateau, native Linux behavior, Firefox/Safari behavior,
or production HTTP buffering/header-deadline guarantees. Benchmark sample arrays
add intentional bounded memory overhead. CPU/RSS observations are short-run
process-tree samples and cannot substitute for those later validations.

## Reproduce

Follow the [experiment setup](../experiments/web-transport/README.md), build the
release experiment, and run from the repository root:

```sh
node experiments/web-transport/benchmark.mjs --hz 10 --clients 0,1,4 \
  --profiles representative,large-counters --pairs 5 --seconds 5 --warmup 1 \
  --output experiments/web-transport/results/local-10hz.json
node experiments/web-transport/benchmark.mjs --hz 100 --clients 1,4 \
  --profiles representative,large-counters --pairs 5 --seconds 3 --warmup 1 \
  --output experiments/web-transport/results/local-100hz.json
node experiments/web-transport/pipeline.mjs --samples 25
RUN_HEADED_TESTS=1 npm --prefix experiments/web-transport test
```

The read-only snapshot subscription and direct-notification experiment publisher
are now [implemented and measured](SNAPSHOTS.md). The production web frame contract
is the next planned increment.

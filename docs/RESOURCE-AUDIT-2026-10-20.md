# Local Apple capture and browser resource audit

Measured October 20, 2026, 18:38–18:43 America/Los_Angeles, on Apple M3,
macOS 27.0 (26A428), with headed Google Chrome 156.0.8078.47. This is a short
diagnostic run, not a long-duration leak certification or a throughput benchmark.

Source: `38ab7e12d6c67b58d54d96cf2b7c5e8a28220c04`, clean workspace.
Release binary SHA-256:
`6e5236d6b614063e62116e28858edc601a1602e97c075166967201023847ed2e`.
Built with `cargo build --release --locked` and default features/settings.

## Measurements

Ran `target/release/tell-tail --apple-unified --output web`, which owns
`/usr/bin/log stream --style ndjson --level info --type log`. Playwright launched
a separate, headed Chrome profile with one page at 1280 × 900. The same browser
first displayed a blank page, then the dashboard. No raw Apple log records were
saved. The 256-second session processed 103,650 records.

CPU is the change in cumulative process CPU time divided by wall time;
**100% means one CPU core**. Memory is resident set size (RSS), sampled roughly
once per second with `ps`. Values below are phase averages for CPU and phase
medians for RSS. Browser values sum the whole isolated browser process tree,
including browser, renderer, GPU and utility processes; shared resident pages can
be counted more than once. They are not the dashboard's JavaScript heap or the
incremental cost of a tab in an already-running browser.

| Phase | Seconds | Tell-Tail CPU / RSS MiB | Apple capture CPU / RSS MiB | Chrome CPU / summed RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| Capture with blank browser | 20.7 | 1.35% / 3.09 | 5.27% / 4.97 | 2.37% / 957 |
| Dashboard warmup | 65.9 | 1.30% / 3.42 | 4.03% / 5.14 | 35.10% / 1,139 |
| Dashboard after warmup | 120.4 | 0.93% / 3.45 | 2.22% / 5.31 | 34.72% / 1,016 |
| Frozen dashboard, ingestion continues | 20.6 | 0.97% / 3.48 | 2.13% / 5.39 | 3.63% / 1,004 |
| Live dashboard after 40 reloads | 20.6 | 0.87% / 3.64 | 1.55% / 5.39 | 38.85% / 1,047 |

Actual system activity varied: the first three phases processed approximately
849, 528 and 277 records/s, respectively (0.89, 0.56 and 0.28 MiB/s).
Capture CPU therefore cannot be compared as a fixed-workload experiment. Browser
startup and allocator caches also affect the blank-page memory comparison.
Chrome's steady phase median was about 59 MiB above the blank-page median;
warmup was about 182 MiB above it. Neither difference isolates tab memory.

## Leak checks

Tell-Tail RSS stayed exactly 3.453 MiB throughout the 120-second steady phase.
After reload churn it settled at 3.641 MiB. Apple capture stayed below 5.5 MiB.
The monitor had 10 numeric file descriptors before and after reload churn.
Each reload waited for a live connection. Both the owned capture process and
monitor were absent after normal SIGTERM shutdown.

Chrome DevTools Protocol forced garbage collection before each heap observation:

| Observation | Used JS heap, bytes | Documents | DOM nodes | JS event listeners |
| --- | ---: | ---: | ---: | ---: |
| Full history after warmup | 1,622,004 | 1 | 869 | 22 |
| After 120 seconds live | 1,664,760 | 1 | 869 | 22 |
| Frozen | 2,386,940 | 1 | 872 | 35 |
| After 40 reloads and settling | 1,780,656 | 1 | 869 | 22 |

Playwright click handling can install instrumentation/listeners in the page;
the frozen measurement includes that interaction. The final reloaded page returns
to the initial DOM/listener counts. Each observation had 480 HTML/SVG elements,
60 chart dots and 60 history rows. No uncaught page errors occurred.

`leaks <monitor-pid>` reported **0 leaks / 0 leaked bytes** both during warmup
and after reload churn. The final scan reported 386 malloc nodes / 234 KiB and
a 2.5 MiB physical footprint. macOS also warned that the process was not debuggable
and inspection was restricted; this result is supporting evidence, not exhaustive
proof. The JS heap checks likewise cannot exclude browser-native leaks.

Code review found bounded record framing and capture diagnostics, a fixed stats
ring, latest-value snapshot publication, bounded HTTP/SSE admission, reaped
connection tasks, explicit child cleanup, and bounded webhook/output queues.
Browser state keeps latest/displayed frames and replaces history DOM rather than
appending indefinitely. No unbounded retention path was identified.

Validation passed: all 98 Rust tests (`cargo test --locked`) and all five browser
frame-contract tests (`node --test browser-tests/frame.test.mjs`). The live browser
exercise used installed Chrome because the Playwright version's bundled browser
was absent. The full existing browser suite was not run as part of this audit.

## Finding and next improvement

No sustained memory-growth or resource-cleanup failure was observed. A longer
soak is needed to detect small or infrequent leaks, especially under other output
modes, alert delivery and maximum configuration. Existing one-hour TUI evidence
in `VALIDATION-0.2.md` concerns an earlier frozen candidate and does not certify
this browser implementation.

The visible dashboard has a meaningful CPU cost. Freezing cuts Chrome CPU by
about 90% while capture continues at a similar rate. This implicates rendering;
the chart in `web/render.mjs` schedules a full-second animation every second,
rebuilds SVG paths on animation frames, and searches vertices for each dot.
This is a likely optimization target, rather than evidence of a memory leak.
Reduced-motion settings already disable interpolation, but their CPU savings
were not measured here. No runtime code was changed during this audit.

Local diagnostic artifacts (ignored by Git) are under `work/resource-audit/`:
`profile.mjs`, `measurements.json`, `progress.txt`, `rust-tests.txt`,
`frame-tests.txt`, `leaks-warmup.txt`, and `descriptors-before-churn.txt`.

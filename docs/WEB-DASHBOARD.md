# Embedded metrics dashboard

Implemented October 20, 2026. The dashboard uses the production
[frame contract](WEB-FRAMES.md) and [loopback server](WEB-SERVER.md).

```sh
tell-tail app.log --from start --output web --open
tell-tail --apple-unified --output web --open  # macOS system activity
my-app 2>&1 | tell-tail --output web
```

The printed `http://127.0.0.1:<port>/` URL works in a browser without Node, an
internet connection, or a frontend server. `--web-port 8765` selects a fixed port;
the default lets the OS choose. Normal stdin EOF retains final metrics until
Ctrl+C. Browser closure never stops ingestion. Only a bounded set of recent
error/fatal previews is retained; this is not a persistent log archive.
The [Apple unified source](APPLE-UNIFIED.md) starts and owns native live capture;
it does not need piped stdin or `jq`.

## Display

- Source, input state, elapsed time, and exact session entry/byte totals.
- Rolling entries/s, MiB/s, warning ratio, and error-plus-fatal ratio. The observed
  window duration is shown alongside the configured duration during warmup.
- Throughput for up to 60 completed process-elapsed seconds, including idle
  seconds. A monotone cubic curve connects samples without overshooting them;
  one-second transitions animate at the display refresh rate (typically 60 fps).
  Each snapshot replaces the chart history. Expand **View exact chart
  values** for a table with exact counts and elapsed intervals.
- Severity counts and distribution for the entire session. Session ratios are
  separate from the rolling ratios above.
- The latest 20 error/fatal previews, newest first, with processing time and
  exact session entry numbers. **Fatal only** filters this retained sample;
  **Freeze display** holds the list for review. Each preview is capped at
  512 UTF-8 bytes with shortened records marked, and controls are replaced with
  spaces. JSON inputs show a preview of the original record. Previews stay in
  memory and are absent from CLI JSON, text, Prometheus, and webhook payloads.
- Expandable ingestion diagnostics: parse failures, oversized and discarded
  partial records, skipped/pending/backlog bytes, and rolling entries/bytes.

Empty ratios and unavailable backlog use an em dash; zero remains zero. Decimal
counter strings are formatted through BigInt, including u64::MAX. SVG coordinates
and displayed percentage/rate rounding are approximate; count labels stay exact.
Source names use DOM text APIs. The layout adapts down to 320 px; controls and
disclosure elements are keyboard accessible with visible focus indicators. Status
messages use text as well as color and announce connection transitions politely.

The palette follows the browser's light/dark preference using Ding's shared
color tokens: warm ivory or graphite surfaces, pine or original Ding green
(`#7EE787`) accents, violet information and unknown states, amber warnings, and
coral errors/fatals. A live connection uses violet; it does not claim that the
logs are healthy. Text labels distinguish every severity, including error from
fatal. The green angular log-tail SVG is used in the masthead and on a graphite
favicon backing. See [the visual foundation](DESIGN.md) for palette roles.
Typography, controls, and panels follow Ding Console's system font stacks,
14 px body, heading hierarchy, 4 px spacing grid, 6 px control corners, and
12 px panel corners. Panels use 24 px insets (16 px on mobile), and buttons
provide 40 px targets with visible keyboard focus.

**Freeze display** holds all displayed metrics and chart values while the live
connection retains one latest frame. **Resume display** paints that frame on the
next animation frame. Connection, capacity, and failure messages continue to
update while frozen. If input completes while frozen, the banner reports the
final state and asks you to resume to view its metrics.

## Connection behavior

The client opens one EventSource to `/api/events`. There is one latest pending
frame, one displayed frame, and at most one scheduled metrics-paint callback.
Synchronous bursts produce one metrics paint; there is no accumulating frame
queue. A separate bounded chart animation interpolates between completed-second
histories for up to one second; it does not add measured subsecond samples.
Freeze, hidden tabs, and reduced-motion preferences stop chart animation. Final
frames, session changes, and gaps in history render immediately.
Generation zero is valid, duplicates/older generations are ignored within a
session, and a new session resets comparison and chart history.

After one second without an advancing generation on a connected live stream,
the banner reports stale data and reconnects. An error closes EventSource before
its automatic retries can start. One snapshot request checks final state and the
advisory available-stream header; subsequent SSE attempts back off from 500 ms to
8 seconds. The snapshot fetch has a two-second deadline. Last values remain
visible, and a disconnect alone never reports completion.

Four live streams are supported. A fifth tab obtains a current snapshot, reports
capacity, and waits for **Retry connection** after another stream closes. It does
not retry automatically. An invalid/unsupported frame or terminal publisher
failure closes the stream and shows a clear error with manual retry.

Hiding the tab closes its stream, cancels pending paint/retry/fetch work, and
retains only current display/latest values. Showing it reconnects for current
state, including while frozen. Capacity and invalid/failure states continue to
require manual retry. A final `complete`, `stopped`, or `failed` frame closes
EventSource and retains final values without reconnecting. Reloading after
normal EOF retrieves the server's retained final frame.

## Browser opening

`--open` requires `--output web`. It passes only the bound dashboard URL as one
argument to `open` on macOS or `xdg-open` on Linux, without a shell, after the
listener and initial frame are ready. Standard input/output/error are redirected
to null, and the opener gets its own process group. Spawn/exit failures produce a
bounded diagnostic and leave the printed URL usable.

The server never waits for an opener to finish. It polls/reaps an exited child
and leaves a still-running desktop process alone at monitor shutdown; some
desktop openers can stay alive with their application. This preserves prompt
shutdown without closing the user's browser. Fake openers cover success, missing
command, nonzero exit, and a long-running handoff without opening a real browser.

## Implementation and checks

`web/` contains the HTML, CSS, small ES modules with JSDoc frame types, and SVG
logo/favicon. Rust embeds only fixed routes with explicit MIME types under the
existing security headers and CSP. There is no framework, chart library, font download,
CDN, or generated frontend bundle. Cargo includes these assets in source
packages; development browser tooling is excluded.

Development checks use a separate pinned Playwright package (Node 22+):

```sh
cargo build --locked
npm --prefix browser-tests ci --ignore-scripts
cd browser-tests
npx --no-install playwright install chromium
npm test
# In a desktop session; Linux CI uses xvfb-run -a:
RUN_HEADED_TESTS=1 npm test
```

`node --test browser-tests/frame.test.mjs` runs the three pure contract/formatting
tests without Playwright. Set `TELL_TAIL_BIN` relative to the repository root to
test a release binary. `PLAYWRIGHT_MODULE_DIR` optionally points at an existing
development installation. The native visibility test uses a disposable browser
profile and CDP without Playwright focus emulation; the default headless run
skips that one test.

Local macOS validation passed 12 JavaScript/browser tests, including native
visibility, against debug and release binaries, plus 84 default, 61 CLI-only,
and 67 notification-only Rust tests. Clippy passes with warnings denied in all
three modes. An extracted source package installs offline and serves all six
dashboard routes and exact final metrics. See the
[validation record](validation/web-dashboard-2026-10-20.json).

CI is configured for the browser suite on Linux and the Rust/package matrix on
macOS/Linux; remote CI has not run here. Firefox/Safari checks, a one-hour browser
resource soak, and production ingestion/freshness performance gates remain open
in [increment 6](PLAN-WEB.md#delivery-increments).

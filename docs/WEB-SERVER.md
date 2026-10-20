# Production loopback server and lifecycle

Implemented October 20, 2026. `--output web` serves the production
[web frame contract](WEB-FRAMES.md) over loopback HTTP and SSE. The root page serves
the [embedded dashboard](WEB-DASHBOARD.md); `--open` optionally launches it in the
default browser.

## Run

```sh
tell-tail app.log --from start --output web
my-app 2>&1 | tell-tail --output web
tell-tail app.log --output web --web-port 8765
```

Tell-Tail prints `http://127.0.0.1:<port>/` through its bounded stderr diagnostic
path. Port zero, the default, asks the OS for an available port. `--web-port`
accepts 0 through 65535 and requires explicit web output. Binding happens before
input is opened or ingestion/alert workers start. An occupied requested port exits
with code 1. Invalid arguments exit with code 2.

Web mode emits no stdout snapshots and uses native approximately 100 ms snapshot
notifications, independently of `--interval-ms`. Each notification encodes once,
outside the ingestion lock, into shared immutable JSON and SSE bytes. HTTP handlers
reuse those bytes; each subscriber sees the latest available generation.

Normal stdin EOF retains exact final metrics, prints a completion notice, and
keeps the server available until Ctrl+C. Following a file continues across its
supported rotation/reappearance behavior. Closing a browser or SSE connection does
not stop ingestion. `--once --output web` is rejected because finite scans promise
exit at EOF. Alerts continue observing native snapshots in web mode.

## Routes and events

| Request | Result |
| --- | --- |
| `GET /` | Embedded dashboard HTML |
| `GET /dashboard.css`, `/dashboard.mjs`, `/frame.mjs`, `/render.mjs`, `/favicon.svg` | Fixed embedded assets, with explicit content types |
| `GET /api/snapshot` | Current complete web schema 1 frame as JSON |
| `GET /api/events` | Current frame immediately, followed by newer complete frames |
| Unknown path | 404 |
| Other methods, including HEAD | Rejected with 405 |

Requests with bodies, transfer encoding, upgrades, or absolute-form targets are
rejected. JSON and SSE are uncompressed; responses use `Cache-Control: no-store`.
There is no HTTP Prometheus scrape route, filesystem browser, WebSocket route, or
mutable API.

Successful snapshot responses include `X-Tell-Tail-Streams-Available` (0 through
4). This advisory count lets a browser diagnose stream capacity after EventSource
reports an opaque error, without consuming a stream slot or changing the frame
schema. Admission remains authoritative; another tab can take a slot immediately.

To inspect the server using a fixed port:

```sh
curl http://127.0.0.1:8765/api/snapshot
curl --no-buffer http://127.0.0.1:8765/api/events
```

Each SSE frame has this framing; `data` is the complete web JSON:

```text
retry: 500
event: snapshot
id: <session_id>:<publication_generation>
data: <web frame JSON>

```

`Last-Event-ID` does not request replay. Every reconnect starts with current state,
so generation gaps are expected. A final frame is sent using the same shape and
then its SSE response ends. Consumers must close EventSource on final state to
avoid its automatic reconnect. A new connection after normal EOF immediately gets
the retained final frame. A 15-second `: heartbeat` comment keeps a quiet live
stream open without changing sequence numbers or metric timestamps. The 500 ms
retry hint is sent. The dashboard owns retries instead: it closes EventSource on
error, checks one snapshot for capacity/finality, then backs off from 500 ms to
8 seconds. Capacity requires a manual retry; see the dashboard guide.

An ingestion error produces a normal final frame with state `failed`. If encoding
or server failure makes such a frame unavailable, existing SSE consumers get a
best-effort terminal `failure` event with the bounded JSON
`{"message":"web output failed"}`. This is a transport control event, separate from
web frame schema 1. It must not be decoded as a metrics frame. The publisher fails
instead of silently serving stale metrics. Unexpected ingestion closure without a
final publication follows this failure path.

## Bounds and local access

| Resource | Limit / behavior |
| --- | --- |
| Accepted HTTP connection tasks | 16; excess accepted sockets close before another task is spawned |
| Active SSE responses | 4; excess streams receive 429 with `Retry-After: 1`; snapshot requests remain available |
| Request headers | 8 KiB including the request line, at most 64 fields, two-second total read deadline |
| JSON frame | 64 KiB serialized UTF-8, enforced by the frame encoder |
| HTTP I/O buffer | 8 KiB buffering threshold; one bounded body chunk can exceed that threshold |
| Socket send buffer | 64 KiB requested from the OS; the OS may adjust its allocation |
| Pending socket write | Closed after two seconds without a successful write; successful partial writes reset this deadline |
| Final delivery / server drain | At most 250 ms after shutdown begins; no browser acknowledgment required |

The listener binds only `127.0.0.1`. Host must match that bound address and port;
on HTTP port 80, its canonical form without `:80` is also accepted. Any supplied
Origin must match the same HTTP origin, with the corresponding default-port
normalization. Foreign, duplicate, and `null` Origin values are rejected. No CORS
access grant is sent. Responses include a restrictive content security policy,
frame blocking, `nosniff`, same-origin resource policy, and no-referrer policy.

Each stream holds its current bounded body chunk plus the shared latest frame,
without a per-client snapshot queue. SSE framing can add the bounded session ID
to the JSON size. Hyper may buffer a bounded amount of encoded output; this is
separate from a frame queue. Kernel/browser buffers can still contain older bytes,
so the dashboard must handle freshness and reconnect on visibility return.

The write deadline wraps the actual socket beneath Hyper. Polling the HTTP body
is not treated as network progress. Request/header work, blocked sockets, and
per-client serialization never run on the ingestion thread. HTTP keep-alive is
disabled: each snapshot request closes after its response, while SSE retains its
one connection until final state, disconnect, timeout, or shutdown.

## Shutdown and build modes

SIGINT/SIGTERM share the existing ingestion signal flag. A 10 ms control tick
observes that flag for shutdown; it does not poll or delay metrics publication.
The server stops accepting connections, gives existing streams up to 250 ms to
send final state, then aborts and drains connection tasks. Ingestion is stopped
and joined. Signal exits remain 130/143; fatal input/server errors exit 1. Native
webhook delivery retains its separate bounded shutdown allowance. Tests enforce
the existing one-second overall shutdown target, including stalled sockets.

Default builds enable the `web` feature. Only web mode starts a single-threaded
Tokio runtime. CLI-only builds exclude the server's active HTTP/runtime dependencies:

```sh
cargo build --release --locked
cargo build --release --locked --no-default-features
cargo test --locked --no-default-features --features snapshot-notifications
```

The last command checks the independent notification API with only Tokio sync
enabled. Browser tooling is development-only; the installed binary needs no Node,
external assets, CDN, or separate frontend server. Fuzzing and the isolated
transport experiment explicitly disable default web features on their library
dependency.

## Verification

At the server milestone on macOS / Rust 1.88.0, all 82 default, 61 CLI-only, and
67 notification-only Rust tests pass, with Clippy warnings denied in all three
modes. Tests exercise live
stdin/file input, EOF retention/reconnect, SIGINT/SIGTERM, occupied ports, input
failure, alerts, Host/Origin/body rejection, malformed/oversized/slow headers,
connection/stream capacity and release, quiet heartbeats, encoder failure, and
unexpected worker closure.

The undrained-socket test uses 60,000 bytes of valid source text and a small client
receive buffer, without a production padding option. Test-only probes around the
real `TcpStream::poll_write` verify successful bytes, a partial socket write, a
blocked write, and deadline expiry. It then admits four new streams, checks current
snapshots from another client, and shuts down with those sockets undrained.

An extracted source package installs offline from cached Rust dependencies and
passes CLI JSON and production web smoke checks, including final state retained
after EOF and signal exit 143. CI is configured to run all three modes on macOS
and Linux; remote CI has not been run here. See the
[validation record](validation/web-server-2026-10-20.json) and
[dependency review](DEPENDENCIES.md). Current dashboard checks are recorded in the
[dashboard guide](WEB-DASHBOARD.md). One-hour resource validation and
maximum-ingestion overhead gates remain later milestones.

# Dependency review

The application stays on Rust 1.88.0. Development tooling is separate: sanitizer
fuzzing uses nightly-2026-10-14 and cargo-fuzz 0.13.2; RustSec checks use
cargo-audit 0.22.2. Install audit tooling with:

```sh
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --json
```

The October 14, 2026 review of the application lockfile found zero advisories and
zero informational warnings across 154 lockfile dependencies, including the
webhook HTTP/TLS dependencies. RustSec database
commit: `edb58f5974b83eb8302332821cb6d389777aa351` (1,329 advisories). There are no
ignored advisories. Rerun at candidate validation: this is dated evidence, not a
promise about future advisories. Audit source: [RustSec](https://rustsec.org/).

The October 20, 2026 notification-feature review found zero vulnerabilities and
zero warnings across 156 locked packages, using RustSec database commit
`b19b2c83c2493961ccb12f9fc4699a00fb2c6c44`. The opt-in
`snapshot-notifications` feature adds Tokio 1.52.3 (MIT, Rust 1.71 minimum),
with only `sync` enabled, and pin-project-lite 0.2.17 (Apache-2.0 OR MIT).
At that review, default builds excluded both from their active dependency graph.
The snapshot API starts no runtime; consumers of its subscription supply an executor.

A later October 20 audit picked up RUSTSEC-2026-0382 and RUSTSEC-2026-0381 in
the existing webhook TLS dependencies. All three lockfiles now pin the patched
`rustls-webpki` 0.103.16 and `webpki-roots` 1.0.10. The updated application graph
has zero vulnerabilities and warnings across 176 packages with RustSec database
commit `e6f94f3760d5e6c54c1b05f204002263bc025524` (1,348 advisories). Webhook and
alert tests pass on Rust 1.88.0. No advisories are ignored.

Declared direct dependency licenses from `cargo metadata --locked`:

| Dependency | Locked version | Declared license |
| --- | --- | --- |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| crossterm | 0.29.0 | MIT |
| memchr | 2.8.3 | Unlicense OR MIT |
| ratatui | 0.30.2 | MIT |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| signal-hook | 0.3.18 | Apache-2.0/MIT |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| ureq | 3.4.2 | MIT OR Apache-2.0 |
| proptest (tests) | 1.12.0 | MIT OR Apache-2.0 |
| tokio (notifications / web) | 1.52.3 | MIT |
| axum (web) | 0.8.9 | MIT |
| bytes (web) | 1.12.1 | MIT |
| futures-util (web) | 0.3.34 | MIT OR Apache-2.0 |
| hyper (web) | 1.12.0 | MIT |
| hyper-util (web) | 0.1.21 | MIT |

The default-enabled `web` feature activates these pinned HTTP dependencies and
Tokio I/O, time, macros, and a single-thread runtime. Axum uses only `http1` and
`tokio`; Hyper uses only `http1` and `server`; hyper-util uses `tokio` and `service`.
No WebSocket, HTTP/2, compression, or additional TLS stack is enabled for the
loopback listener. Only `--output web` starts its runtime. The active normal
dependency graph for `--no-default-features` excludes Axum, Hyper, and Tokio;
`--no-default-features --features snapshot-notifications` keeps only Tokio sync.
Fuzz and experiment manifests explicitly disable default web features. All three
build modes pass on Rust 1.88.0; see [server validation](WEB-SERVER.md).

The screen-observation script uses pyte 0.8.2 (LGPLv3 package classifier) and wcwidth
0.2.13 (MIT) in a development virtual environment. Neither is linked into the
Rust binary or included in the source package. Their only role is interpreting
terminal output during validation.

Dashboard assets are plain HTML, CSS, JavaScript modules, and SVG, embedded with
`include_str!`; there are no frontend runtime dependencies, external fonts, or
build tools. Development browser tests in `browser-tests/` pin Playwright and
playwright-core 1.62.1 (Apache-2.0), with optional fsevents 2.3.2 (MIT), in their
own npm lockfile. They require Node 22 or later and a development browser download;
none is linked into the binary or included in the Cargo source package. `--open`
uses the system `open` (macOS) or `xdg-open` (Linux) command without a shell.

Webhook delivery uses [ureq 3.4.2](https://docs.rs/ureq/3.4.2/ureq/) with default
features disabled and only the `rustls` feature enabled. Its Rust 1.85 minimum
fits the existing 1.88 toolchain. Requests run in a supervised helper process;
the parent enforces a deadline even if name resolution stalls. HTTP responses
have a 16 KiB header limit and their bodies are not consumed. Each queued payload
is at most 64 KiB, and each helper request is at most 128 KiB. The fuzz workspace
lockfile includes the same new dependencies.

Tell-Tail is licensed under the [MIT License](../LICENSE), with source hosted at
[ding-labs/tell-tail](https://github.com/ding-labs/tell-tail). Before public binary
redistribution, prepare the selected dependency notices and verify the archive's
contents. No registry publication or binary release has been performed.

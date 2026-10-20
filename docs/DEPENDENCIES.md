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
Default builds exclude both from their active dependency graph. The library
starts no async runtime; an enabled consumer supplies its own executor.

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
| tokio (optional snapshot notifications) | 1.52.3 | MIT |

The screen-observation script uses pyte 0.8.2 (LGPLv3 package classifier) and wcwidth
0.2.13 (MIT) in a development virtual environment. Neither is linked into the
Rust binary or included in the source package. Their only role is interpreting
terminal output during validation.

Webhook delivery uses [ureq 3.4.2](https://docs.rs/ureq/3.4.2/ureq/) with default
features disabled and only the `rustls` feature enabled. Its Rust 1.85 minimum
fits the existing 1.88 toolchain. Requests run in a supervised helper process;
the parent enforces a deadline even if name resolution stalls. HTTP responses
have a 16 KiB header limit and their bodies are not consumed. Each queued payload
is at most 64 KiB, and each helper request is at most 128 KiB. The fuzz workspace
lockfile includes the same new dependencies.

Tell-Tail is licensed under the [MIT License](../LICENSE), with source hosted at
[zuchka/tell-tail](https://github.com/zuchka/tell-tail). Before public binary
redistribution, prepare the selected dependency notices and verify the archive's
contents. No registry publication or binary release has been performed.

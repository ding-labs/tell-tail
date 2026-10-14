# Fuzzing Tell-Tail

The fuzz workspace uses nightly-2026-10-14 and cargo-fuzz 0.13.2; the application
continues to build with Rust 1.88. LLVM/C++ build tools are required.

From the repository root:

```sh
rustup toolchain install nightly-2026-10-14 --profile minimal
cargo +nightly-2026-10-14 install cargo-fuzz --version 0.13.2 --locked
cargo +nightly-2026-10-14 fuzz build
export ASAN_OPTIONS=quarantine_size_mb=32
cargo +nightly-2026-10-14 fuzz run framing -- -max_total_time=1800 -timeout=5 -rss_limit_mb=512 -max_len=65536
cargo +nightly-2026-10-14 fuzz run classification -- -max_total_time=1800 -timeout=5 -rss_limit_mb=512 -max_len=65536
cargo +nightly-2026-10-14 fuzz run timed_engine -- -max_total_time=1800 -timeout=5 -rss_limit_mb=512 -max_len=65536
```

The 32 MiB ASan quarantine keeps sanitizer bookkeeping within the 512 MiB fuzz
process budget; this is separate from the application RSS limits. A preliminary
framing campaign exhausted that budget with the default quarantine, so isolated
replay and fresh bounded campaigns were run before recording success. The saved
input replayed 10,000 times successfully. The fresh framing campaign completed
950,997 executions in 31 seconds at 115 MiB RSS; timed engine completed 257,982
at 107 MiB. Classification completed 1,422,227 executions in 31 seconds at
355 MiB with the default quarantine. These macOS arm64 smoke runs are not the
30-minute release gates.

Use 30 seconds per target for a smoke run. Preserve the workspace lockfile and
record the application commit, toolchain, target, elapsed time, and executions.
Run the targets sequentially for reproducible resource accounting.

Framing compares emitted lines with an independent split-based oracle.
Classification compares decoded JSON fields and syntax with serde_json Value,
accounting for Tell-Tail's stricter duplicate-level policy and the reference
parser's stricter numeric range/depth limits. Timed engine operations check
monotonic counts, severity totals, byte accounting, and unknown diagnostics through
feeds, idle periods, EOF, and generation-boundary discards.

Named `seed-*` inputs are synthetic and committed. Generated coverage corpus and
crash artifacts are ignored. Reproduce a finding by passing its artifact path as
the final argument to `cargo fuzz run <target> <artifact>`, minimize with
`cargo fuzz tmin <target> <artifact>`, and add a normal deterministic regression
with the fix. Never add raw production logs to the corpus.

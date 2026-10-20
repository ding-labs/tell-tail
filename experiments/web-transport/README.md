# Web transport experiment

Development-only SSE/WebSocket comparison for [the local UI plan](../../docs/PLAN-WEB.md).
This isolated Cargo workspace reuses Tell-Tail's library without adding web
dependencies, routes, or assets to the release binary/package.

Run Rust checks from the repository root:

```sh
cargo test --manifest-path experiments/web-transport/Cargo.toml --locked
cargo clippy --manifest-path experiments/web-transport/Cargo.toml --locked --all-targets -- -D warnings
```

Frames use actual engine statistics with 60 completed history buckets. The
`large-counters` profile deliberately widens cumulative counters/history to u64
limits; it is a payload-width stress case, not a consistent ingestion ledger.
Counter strings avoid losing integer precision in the browser. Both transports
share the same JSON bytes, serialized once per publication. This experiment's
envelope is separate from the future production web contract and CLI schema 2.

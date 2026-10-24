# Static runtime notices

Rust 1.88.0 bundles musl 1.2.3 for the distributed Linux musl targets. See the
upstream [musl build script](https://github.com/rust-lang/rust/blob/1.88.0/src/ci/docker/scripts/musl.sh)
and [musl toolchain script](https://github.com/rust-lang/rust/blob/1.88.0/src/ci/docker/scripts/musl-toolchain.sh).

The vendored COPYRIGHT file and source notice comments come from the official
<https://musl.libc.org/releases/musl-1.2.3.tar.gz> archive, SHA-256
`7d5b0b6062521e4627e099e4c9dc8248d32a30285e959b7eecaa780cf8cfd4a4`.
SOURCE-NOTICES collects upstream C-style comment blocks containing copyright,
permission, redistribution, license, or public-domain notices, with source paths.
The files preserve both the overall musl notice and individual source notices.

Linux release archives include both files. Review this bundle whenever the Rust
release compiler or static C runtime changes; packaging rejects an unreviewed
compiler version. The Rust standard-library copyright file remains separate.

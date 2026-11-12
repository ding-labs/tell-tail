# Dependency advisory review for 0.4.0

Reviewed November 12, 2026 for candidate
`09df02f7c233b819b94c08c6229b2ea0934ec206` and its three advertised archive targets.
The [exact-source audit](https://github.com/ding-labs/tell-tail/actions/runs/43307355126)
reports zero vulnerability-class findings and two informational unsoundness
advisories. It is not a zero-warning audit. Database commit:
`af7b3ec33460b8e2de8d422e16d628518891a519` (1,490 advisories).

## Scope and disposition

Neither affected API is used by Tell-Tail or a caller in its resolved normal
runtime dependency sources. The candidate can proceed with these explicitly
reviewed, unused-API findings; no advisory ID is added to an ignore list and no
source or dependency change is included in this frozen release. This conclusion
is specific to this candidate and these APIs, not a general claim that the
packages are free of defects.

| Advisory | Affected dependency | Candidate use and evidence |
| --- | --- | --- |
| [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html) | rustix 1.1.5 extended-attribute getters/list functions with a zero-length buffer | Tell-Tail uses file descriptors, readiness, flags, files, and socket options. There are no calls to the affected xattr functions in its source or normal dependency consumers. Consumer search hits are libc FFI declarations and linux-raw-sys syscall constants, not rustix calls. |
| [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html) | Tokio 1.52.3 `AsyncFd` constructors accepting arbitrary `AsRawFd` objects | Tell-Tail does not use `AsyncFd`. Web connections use owned `tokio::net::TcpStream`/`TcpListener`, whose implementations use `PollEvented` over Mio sockets. Other normal dependency sources contain no AsyncFd use. Tokio's remaining references outside its AsyncFd implementation are documentation/reexports. |

The review used `cargo metadata --locked` plus `cargo tree --locked --edges normal
--target TARGET` for Apple ARM64 and both Linux musl targets, then searched the
resolved `src` directories and Tell-Tail source for affected symbols. The union
contains 126 normal package/version entries. The provider implementations were
reviewed separately so their declarations and examples were not mistaken for
application callers. CLI-only and notification-only builds use subsets of the
default runtime graph. This is source review, not a formal call-graph proof.

The raw audit, resolved metadata, per-target dependency trees, and search results
are preserved under ignored `work/release-0.4.0/` in the validation checkout. The
[CI summary](ci-summary.json) records the audit checksum and database identity.

## Follow-up

Update rustix to a patched version (1.1.6 or later) and move the pinned Tokio
version to a patched supported release in a focused maintenance increment, with
Rust 1.88 compatibility, all feature configurations, packaging notices, and
relevant I/O regression checks. Reassess this disposition if code or dependencies
start using xattr or AsyncFd, especially during Linux device-adapter work. The
advisory review must be repeated for the next release candidate; this record is
not a standing exception.

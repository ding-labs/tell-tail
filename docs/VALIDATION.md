# Tell-Tail MVP validation

Validated on October 14, 2026. The implementation supports file following and
stdin, bounded framing, severity classification, rolling statistics, the terminal
dashboard, text/NDJSON reports, and Prometheus stdout or atomic metrics files.

## Correctness and platform checks

- Formatting and clippy pass with warnings denied.
- 22 Rust tests pass on macOS: 15 unit tests and 7 executable integration tests.
- The same tests pass on Linux in the official Rust 1.88 Bookworm container.
- The project also type-checks all targets for x86_64-unknown-linux-gnu.
- Pseudo-terminal checks pass on macOS and Linux for piped logs, separate keyboard
  input, display freeze, help, resize, EOF, `q`, SIGTERM, and terminal restoration.
- A deliberately stalled output socket exits promptly on cancellation: observed
  17 ms on macOS and 20 ms on Linux, with an explicit output failure.
- Prometheus exposition passes `promtool check metrics`. Tests cover all severity
  series, complete atomic snapshots, missing-source recovery, freshness updates,
  orderly shutdown, and rejecting aliases of the input as output destinations.
- Ingestion tests cover arbitrary read boundaries, partial records, CRLF, invalid
  UTF-8, malformed JSON, false-positive severity words, oversized records,
  observed truncation, rename rotation, disappearing paths, and idle windows.

The Linux container used a read-only bind mount of the repository and built an
independent copy inside the container. No host build artifacts were reused.

## Cached-file benchmark

Host: Apple M3, arm64, macOS 27.0. Compiler: rustc 1.88.0. Release settings: thin
LTO, one codegen unit, stripped binary. The resulting local executable is about
1.2 MiB. These measurements use simple repeated records and warmed file caches;
they do not represent uncached disk speed or arbitrary production log formats.

```sh
cargo build --release --locked
python3 scripts/benchmark.py --records 2000000 --repeats 5
```

One warmup pass precedes five timed subprocess runs. Each run verifies the exact
record count, severity count, and consumed-byte count. Wall time includes process
startup and final report output.

| Fixture | Median records/sec | Median MB/sec | Observed records/sec range |
| --- | ---: | ---: | ---: |
| 200-byte INFO text | 10,325,896 | 2,065.2 | 5,981,546–11,236,605 |
| 500-byte INFO JSON | 980,867 | 490.4 | 531,705–1,603,728 |

Both fixtures exceeded the provisional 1 million text / 250,000 JSON records per
second targets on this host. The wide spread makes these preliminary measurements;
repeat on the intended hardware with representative log samples before setting
operating limits.

## Sustained pipe checks

```sh
python3 scripts/soak.py --seconds 60
python3 scripts/soak.py --seconds 60 --tui
```

The generator repeatedly writes 1,000-record batches of 200-byte text records:
90 percent info, 9 percent warnings, 1 percent errors. Every run verifies exact
final counts. RSS is sampled once per second using `ps`, so these are maxima of
samples, not an instrumented maximum over every instant of process lifetime.

| Mode | Records processed | Processing seconds | Records/sec | Maximum sampled RSS |
| --- | ---: | ---: | ---: | ---: |
| NDJSON reports | 464,102,000 | 59.997 | 7,735,434 | 2.58 MiB |
| Dashboard through a PTY | 1,708,589,000 | 59.995 | 28,478,857 | 3.91 MiB |

Both runs stayed below the provisional 64 MiB memory ceiling. TUI samples rose
from about 3.47 to 3.91 MiB during the minute; this short run alone does not prove
indefinite allocator stability. The stored application state is structurally
bounded by record size, window length, and a single current snapshot.

These were sequential runs on an active workstation, with other validation work
occurring during part of the headless run. They establish sustained correctness
and sampled memory use, not a controlled comparison of display overhead. The
under-10-percent dashboard overhead target remains to be measured in an isolated,
paired benchmark. Multi-hour soak testing is also a follow-up release check.

## Deliberate limits

The MVP counts physical lines and has no durable read checkpoints or multiline
reconstruction. Numeric severity profiles, logfmt, multiple inputs, Windows, an
HTTP scrape endpoint, and a web UI are later increments. File truncation/regrowth
between polls and writes to a rotated descriptor after handoff can be missed.
These behaviors and exit statuses are documented in the README.

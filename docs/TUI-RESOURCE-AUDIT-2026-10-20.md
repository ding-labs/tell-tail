# Terminal dashboard resource audit

Checked October 20, 2026, on Apple M3 / macOS 27.0 (26A428).
Source: `fe9fe246ce2ab640c55bc1c0faa519194cf8e864`, clean workspace.
Release binary SHA-256:
`fb62516b184f9d433036a2032b1059829442add34db99cbda859a8cd5750158a`.
No runtime changes were made for this audit.

## Live Apple capture

Ran `target/release/tell-tail --apple-unified --output tui` from approximately
18:54–18:58 America/Los_Angeles. A draining pseudo-terminal provided a controlling
TTY at 100 columns × 26 rows; compact mode used 40 × 10. Stdin was `/dev/null`,
confirming native capture does not depend on stdin. Keyboard input used the TTY.
The session processed 20,407 Apple records / 21,450,530 bytes in 189.174 seconds,
with zero parse failures, oversized records or pending bytes in its final report.

CPU values are cumulative process CPU-time deltas divided by phase wall time;
100% means one core. RSS was sampled approximately once per second. The table
reports phase CPU averages and median RSS. Tell-Tail uses one output helper;
Apple capture is a separate `/usr/bin/log stream` child.

| Phase | Duration | Tell-Tail CPU / RSS MiB | Output helper CPU / RSS MiB | Apple capture CPU / RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| Warmup | 65.0 s | 0.98% / 4.33 | 0.05% / 2.17 | 1.43% / 5.67 |
| Live, full dashboard | 61.0 s | 0.88% / 4.39 | 0.03% / 2.28 | 1.00% / 5.86 |
| Frozen display | 20.1 s | 0.95% / 4.41 | 0.05% / 2.28 | 1.15% / 5.86 |
| Help visible | 10.6 s | 1.04% / 4.42 | 0.00% / 2.30 | 0.95% / 5.88 |
| Compact dashboard | 15.9 s | 0.38% / 4.42 | 0.06% / 2.30 | 1.13% / 5.89 |
| Resumed full dashboard | 15.8 s | 0.89% / 4.42 | 0.00% / 2.36 | 1.07% / 5.89 |

Rounded zero CPU means below this short phase's CPU-clock resolution, not no work.
The steady full dashboard plus output helper used approximately **0.92% of a core
and 6.67 MiB RSS**. Including Apple capture, the corresponding sums were 1.92%
and 12.53 MiB. System log traffic varied naturally during the run.

These measurements exclude Terminal.app or another graphical terminal emulator.
The harness drains bytes without drawing pixels. They therefore measure the
application's terminal output path, not total desktop rendering cost. The separate
synthetic soak overlapped most of this capture; these are diagnostic observations,
not isolated throughput benchmarks or a controlled comparison to browser CPU.

Tell-Tail RSS was exactly 4.391 MiB throughout the steady phase and settled at
4.422 MiB after interactions. The native process tree had 18 numeric descriptors
during live capture; the monitor alone had 10 at the final observation.
`leaks` reported zero leaked bytes, with the same non-debuggable-process inspection
restriction noted in the browser audit. Quit returned status 0 in 260 ms,
restored the alternate screen/cursor, and left none of the three processes alive.
No sustained memory-growth or cleanup failure was observed in this short run.

## Correctness, responsiveness and stress checks

- Both Rust TUI tests passed, including compact diagnostics and terminal sizing.
- `scripts/check_terminal.py` passed keyboard, freeze/help, resize, counts and
  terminal restoration for quit, SIGTERM, SIGINT and EOF. This script uses the
  current debug binary. Observed shutdowns were 153–260 ms.
- `scripts/check_terminal_stall.py` restored terminal settings and stopped the
  application in 762 ms with an intentionally undrained output PTY.
- Release-mode `scripts/check_freshness.py`, using the existing validation virtual
  environment and a Pyte terminal emulator, observed 25 successive records:
  p95 visibility latency 215 ms, maximum 216 ms, quit 226 ms. Freeze/resume,
  help/Escape and compact parse-failure diagnostics also passed.
- A release-mode 150-second seeded burst soak at a configured 1,000 records/s
  processed exactly 147,000 records and 1,574,275,920 bytes. Exact expected counts
  included 29,400 parse failures and 1,470 oversized records. It exercised 18
  terminal actions, peaked at 8.641 MiB combined monitor/helper RSS, and never
  exceeded 13 combined numeric descriptors. Sampled cumulative CPU time was
  1.69 seconds, approximately 1.1% of one core over the run. EOF-to-exit was 38 ms.
  No observation gap exceeded 83 ms.

## Assessment

The TUI already uses Ratatui 0.30.2 and a 200 ms keyboard polling interval, giving
roughly five dashboard updates per second during normal operation. Its sparkline
shows completed seconds without continuously interpolating them. Code review
found bounded chart history, at most one frozen snapshot, shared writer ownership
without an ownership cycle, and an explicit terminal restoration guard.

The terminal path has low measured resource costs and passed the exercised
controls and cleanup checks. There is no evidence here that it needs the browser
chart's animation optimization. Longer soaks remain necessary to exclude slow or
rare leaks; terminal emulator CPU and graphical appearance were not audited.

Local artifacts, ignored by Git, are in `work/tui-audit/`: `profile.py`,
`apple-tui.json`, `apple-progress.txt`, `soak.json`, `soak-progress.txt`,
`unit-tests.txt`, `stall.txt`, `freshness.json`, and `descriptors-live.txt`.

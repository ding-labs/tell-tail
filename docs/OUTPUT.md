# Output isolation

Tell-Tail's terminal, text, JSON, and Prometheus-stdout outputs use one helper
process for stdout. The helper inherits stdout without
changing its status flags. Only newly created private control pipes are made
nonblocking. Metrics-file and web output create no persistent stdout helper;
runtime diagnostics still use the bounded diagnostic path. See the
[web connection and lifecycle bounds](WEB-SERVER.md). Input descriptors duplicated
by Tell-Tail are close-on-exec so the helper cannot retain the log stream.

The parent sends a length-prefixed chunk of at most 64 KiB and waits for an errno
acknowledgment on the helper's stderr. The helper writes the complete chunk before
acknowledging it. Only one chunk is in flight. The parent buffers at most 64 KiB,
and the helper has at most two bounded chunks plus fixed kernel pipe buffers.
No queue of snapshots or raw logs is introduced.

The optional [Apple unified source](APPLE-UNIFIED.md) owns a separate native
`/usr/bin/log` process, independent of output mode. Its stdout/stderr use private
nonblocking pipes; at most 4 KiB of stderr is retained. Include that process's CPU
and RSS when measuring native capture. File/stdin input starts no capture process.

A complete report ends at `flush`. One absolute two-second deadline covers all
chunks since the preceding flush. Cancellation allows up to 250 ms for ready
output to finish, then reports an output failure. The parent kills and reaps the
helper when the writer is dropped. Regular filesystem writes can still be held
in an uninterruptible kernel operation; the deadline is not a universal kernel
I/O guarantee.

The helper's control reader runs separately from its blocking writer. After each
chunk is handed off it waits for the next header. Parent disappearance closes the
control pipe and terminates the helper even if stdout is blocked. The helper
ignores SIGINT/SIGTERM as shutdown requests; the parent coordinates final output.
The internal worker argument is an implementation detail, not a public CLI mode.

An isolated helper avoids both shared `O_NONBLOCK` changes and readiness-to-write
races with another writer. An output thread alone could remain stuck in a write.
Experiments with a blocking child confirmed prompt process teardown; the Rust
implementation is covered by `scripts/check_output.py` for descriptor flags,
normal cancellation, parent death, deadline expiry, and broken downstream pipes.
Darwin exposes an additional kernel-managed flag after a first write, so the
regression checks caller-controlled status flags rather than opaque kernel bits.

The helper adds process startup and IPC overhead. Candidate benchmarks must count
its CPU and RSS as part of Tell-Tail and measure paired dashboard/headless runs.
The existing ingestion architecture and latest-snapshot handoff remain unchanged.

Terminal cleanup stops the original writer before Ratatui's drop handling, then
restores termios and attempts cursor/alternate-screen cleanup through a fresh
bounded writer. Cleanup writes and runtime diagnostics each receive a 250 ms
budget. An undrained terminal may not display escape sequences or diagnostics;
restoring termios and exiting must still complete. Fullscreen initialization uses
explicit clear/move commands instead of a blocking cursor-position query.

The stalled-PTY regression keeps a supervisor alive to inspect termios after the
application exits. It acknowledges application shutdown before closing its own
controlling-terminal descriptor, whose close can drain on Darwin. This separates
the application's shutdown latency from the test supervisor's lifetime.

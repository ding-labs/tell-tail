# Linux log inputs implementation plan

Status: proposed work after the 0.3.0 release. None of the new flags below are
implemented in 0.3.0. This plan defines the behavior, delivery increments, and
acceptance checks needed for Linux monitoring to feel as direct as
`tell-tail --apple-unified` on macOS.

## Outcome and scope

The first deliverable is a native `journalctl` source plus a portable parser for
saved journal JSON. It covers system services, user services, and kernel records
on systemd hosts without `jq`, rewritten logs, a daemon, or a systemd library
linked into the static binary. Subsequent increments cover kernel logging on
hosts without a usable journal and clearer recipes for existing Unix inputs.

| Source | Current behavior | Planned outcome |
| --- | --- | --- |
| Regular files | Follow, retry missing paths, detect observed truncation, drain rotation | Preserve behavior; document `/var/log` permissions and format limits |
| Pipes and redirected stdin | Bounded physical-record framing | Preserve behavior; add system-service, SSH, and FIFO recipes |
| Journal JSON | `PRIORITY` is not recognized | Explicit portable format with journal severity and diagnostics |
| System/user journal | Requires an external command | One owned `journalctl` child, filters, visible failure, bounded cleanup |
| Kernel records in journal | External command required | Native journal source with a kernel filter |
| Kernel ring on non-systemd hosts | No dedicated source | Separate read-only `/dev/kmsg` adapter after journald ships |
| Syslog with `<PRI>` | RFC 3164/5424 parsing already exists | Keep and document; no severity invention when `<PRI>` is absent |
| Docker/Kubernetes | Runtime command pipes; saved envelope support | Keep existing paths and document composition |

This work does not add a UDP/TCP syslog collector, bind `/dev/log`, collect CPU or
process metrics, install a service automatically, merge independently opened
sources, or persist logs/checkpoints. Several units selected in one journal query
are one aggregate source; per-unit series and comparison dashboards are separate
work.

## Proposed commands

```sh
# Live system journal, only records arriving after attachment
tell-tail --journal
tell-tail --journal --output web --open

# One or several system services; repeated units form an OR selection
tell-tail --journal --unit nginx.service
tell-tail --journal --unit api.service --unit worker.service

# Current user's journal and user service
tell-tail --journal --user
tell-tail --journal --user --unit worker.service

# Current-boot kernel records through the journal
tell-tail --journal --kernel

# Portable saved-capture classification; works on macOS too
tell-tail journal.ndjson --format journal --once

# Explicit external capture for more advanced journalctl filters
journalctl -f -n 0 -o json --no-pager | tell-tail --format journal
```

`--journal` selects journal parsing and conflicts with a file, `--demo`,
`--apple-unified`, `--once`, `--from`, an envelope, a JSON profile, and severity
field/path overrides. Only auto or explicit journal format is allowed with the
native source. `--format journal` accepts file/stdin and normal file
options. The first native release accepts no arbitrary `journalctl` argument
string; advanced filters stay in an explicit pipeline.

`--unit` is repeatable, journal-only, capped at 32 entries of 256 UTF-8 bytes.
Reject empty values, control characters, and glob syntax in the first version.
Require explicit unit names including the suffix. This keeps the source label
and unit-selection behavior predictable. It does not claim the stream contains
only messages emitted by the service: journalctl unit queries can include
manager messages about it.

`--user` switches to the user journal and makes units user units. `--kernel`
conflicts with units and user mode and implies current boot. A later
`--journal-boot` selector can accept an explicit boot ID or numeric offset.
Do not add overloaded bare `--boot`, PID, priority, namespace, or arbitrary match
filters until their combination semantics and tests are specified.

Priority filtering is deliberately absent initially: filtering out info records
would change the error-percentage denominator. Users who filter externally must
see that their statistics describe the filtered stream.

## Journal parser contract

The journal represents priority as a decimal string from 0 to 7. Its JSON export
can represent repeated values as arrays and binary values as byte arrays.
`MESSAGE` can contain embedded newlines within a single journal entry.
[Journal fields](https://www.freedesktop.org/software/systemd/man/260/systemd.journal-fields.html)
· [Journal JSON serialization](https://systemd.io/JOURNAL_EXPORT_FORMATS/#journal-json-format)

Implement a separate `Format::Journal`, not a new numeric variant of the Pino
profile. Journal values, envelope semantics, and priority authority differ from
application JSON. Explicit selection avoids reclassifying an unrelated JSON
record that happens to contain `PRIORITY`.

| Journal priority | Tell-Tail severity |
| --- | --- |
| `"0"`, `"1"`, `"2"` | fatal |
| `"3"` | error |
| `"4"` | warn |
| `"5"`, `"6"` | info |
| `"7"` | debug |

Accept exactly those single-character strings. Integer JSON values, padded
strings, floats, signs, booleans, arrays, null, and missing priorities remain
unknown. This strict initial contract matches the native producer; an explicit
future compatibility option may accept normalized integer exports if justified
by real fixtures. Never apply `PRI % 8` to journal `PRIORITY`: unlike a syslog
priority header, this field already contains severity without facility bits.

The selected journal priority is authoritative. A JSON `level` alongside it, or
`ERROR` inside `MESSAGE`, does not override it. A service that logs all stdout
at info priority will consequently show info. Explain that limitation and offer
`journalctl ... -o cat | tell-tail --profile pino` when the user explicitly wants
application-message classification. Do not silently unwrap or parse `MESSAGE`,
which could change entry counts, precedence, and alert behavior.

Use the existing streaming JSON validation rules: validate the entire object,
including ignored Unicode escapes; reject duplicate selected keys as malformed.
A valid object with absent or unsupported priority counts once as unknown. A
malformed object or non-object record in explicit journal mode counts once as
unknown plus a parse failure. Blank physical lines count as unknown. Do not
copy Apple's rule of silently skipping objects with missing severity: ordinary
journal entries without usable priority must remain in the denominator.

One physical JSON line counts once, even if `MESSAGE` contains escaped newlines.
Outer bytes, delimiters, oversized records, partial EOF, and file-generation
boundaries retain current accounting. Unsupported but valid `MESSAGE` arrays do
not prevent classifying a valid scalar priority. Never expand a journal record
into several synthetic records.

Use fixed diagnostic counters for missing, unsupported, and repeated/ambiguous
priority. Retain no arbitrary field keys or values. Distinguish syntax failures
from valid-but-unsupported shapes in the guidance. Default recent-error previews
remain the original bounded record, sanitized by the existing preview code.
A separately reviewed presentation increment may prioritize a bounded decoded
`MESSAGE` preview without changing measured bytes or retaining the full object.

## Native journal source

### Command and platform behavior

Start one child without a shell. The intended base arguments are
`journalctl --follow --lines=0 --output=json --no-pager`. Add `--system` by default,
`--user` for user mode, explicit `--unit=NAME` arguments, or `--dmesg --boot=0`
for the kernel mode. Treat all user values as individual arguments, never shell
text. Resolve the installed executable through the normal command search path;
report a missing executable as a runtime error. Do not install dependencies.

Do not use `--quiet`: it can suppress access warnings. Do not use a pager, colors,
JSON pretty printing, or JSON sequence framing. Do not add `--all` initially:
journalctl's default JSON field-size treatment limits expensive large fields;
null values and the resulting preview limitation must be visible in the docs.
Retain full entry metadata in the producer output initially so custom exports
and live input have the same parsing contract. Future field selection can reduce
bytes, but must explicitly change the byte-throughput description.
The producer's options and serialization behavior are documented in
[journalctl](https://www.freedesktop.org/software/systemd/man/260/journalctl.html).

Choose and test a minimum supported systemd version before implementation closes.
Use only longstanding arguments in the initial path; qualify the actual versions
on at least two systemd distributions rather than assuming the newest man page
is the minimum. Unsupported options must fail visibly, with no fallback that
changes source scope or severity interpretation.

Live capture is Linux-only. The journal parser remains portable and is built
in CLI-only configurations. Non-systemd Linux images can still use files, pipes,
saved journal captures, and the later kernel adapter; `--journal` explains a
missing or unusable journal rather than displaying a healthy-looking zero.

### Permissions, empty streams, and lifecycle

Use the current user's privileges. Never invoke sudo, change group membership,
relax device permissions, or adjust journald settings. Report journal access
warnings without dumping arbitrary diagnostics: sanitize controls, cap retained
stderr at 4 KiB, and drain it continuously with bounded work. An empty accessible
journal or unmatched unit is a waiting state, not proof of service health.
Partial visibility cannot always be detected from journalctl's exit status;
label the scope as the accessible system/user journal and keep warnings visible.

Share a small owned-child implementation with Apple capture only after extracting
and proving the existing contract unchanged. Keep executable/arguments/parser
selection in source-specific adapters. Both private pipes are nonblocking;
stdout and stdin inherited by Tell-Tail retain their original descriptor flags.
The child has null stdin; TUI keyboard input continues through the controlling
terminal.

Required transitions: starting -> reading/waiting -> stopped or failed. An
unexpected child exit, including zero, or closed capture stdout while live is
failed, not ordinary stdin EOF. Propagate a failed final snapshot and return 1.
Do not auto-restart and conceal capture gaps. Normal q/SIGINT/SIGTERM reaps the
child and preserves 0/130/143 semantics, with output failure taking precedence.
Keep the existing <=100 ms input-poll cancellation checks and bounded shutdown
budget; kill and reap a child that does not cooperate. Exercise stderr flooding,
child exit with remaining stdout, and failure during web startup.

File and ordinary stdin behavior remain unchanged. Saved journal scans can finish
normally. Piped journalctl EOF retains web metrics under the existing stdin
contract; only the owned native source knows an unexpected producer exit is a
capture failure. Document this reason to prefer the native source.

### Historical journal input

Keep historical collection in a separate increment. Proposed
`--journal-since VALUE` plus `--journal-once` must resolve the conflict between
zero-history attachment, finite scans, and follow-after-history explicitly.
A finite owned child may exit zero normally; that is a different lifecycle mode.
Historical scan rates still measure processing time. `--once --output web`
remains rejected unless its general contract is separately changed.

Do not persist or accept a cursor automatically in the first version. Later
resume support needs cursor expiry/vacuum handling, restart discontinuities,
source identity, atomic checkpoint writes, and an explicit at-least-once versus
skip policy. Journal rotation is handled by journalctl; Tell-Tail must not claim
losslessness against journal rate limits, vacuuming, producer drops, or access
changes.

## Kernel logging without systemd

Deliver kernel-via-journal first. For non-systemd hosts, add a distinct read-only
`--kernel-log` source backed by `/dev/kmsg`, plus `--format kmsg` for saved raw
records. `--kernel-log` is mutually exclusive with other native sources, files,
envelopes, profiles, and journal filters. Do not reinterpret `/dev/kmsg` as an ordinary file: the kernel exposes
record reads, sequence numbers, nonblocking readiness, and an overwrite error.
[Kernel ABI](https://www.kernel.org/doc/Documentation/ABI/testing/dev-kmsg)

The implementation spike must preserve each kernel read as one record, including
continuation metadata lines. Define a framing adapter at that boundary instead
of passing every embedded newline to the ordinary framer. Parse the leading
priority, sequence, timestamp, flags, and message separator; preserve unknown
future header extensions while validating the required fields. Use low severity
bits from the combined kernel priority; do not impose the journal field's 0..7
whole-value bound.

Open read-only and nonblocking, seek to the end for live attachment, and never
write, clear, or consume another reader's state. Size the bounded read buffer
using the documented ABI and verified kernel behavior; handle undersized-buffer
errors without an infinite retry loop. Track only the previous sequence and
bounded diagnostic counts. On overwrite/EPIPE, report the discontinuity, resume
at the kernel-provided position, and count a known sequence gap separately from
observed records. A gap does not fabricate errors or complete missing records.
Treat unexpected sequence resets conservatively as discontinuities.

For this source, one device record is one counted record; it can contain metadata
continuation lines. State that exception to ordinary physical-line counting in
all source docs. The kernel's `c` fragment flag does not authorize concatenating
unrelated records: initially count each delivered record once and report fragment
presence without reconstruction. Preserve exact device bytes, not a normalized
synthetic line length.

Saved raw kmsg needs a separate bounded framing mode: a validated header starts
a record and leading-space metadata lines belong to it. Finish the previous
record at the next header or finite EOF. Cap the entire assembled record at
`--max-record-bytes`; drain oversized continuation groups and count them once.
Do not flush an incomplete followed capture just because it is temporarily idle.
Malformed headers/continuations and resynchronization need explicit fixtures and
accounting. Ship saved-format support only with this framing contract, not by
routing it through the ordinary newline framer. The native device path already
has record boundaries and must not wait for the next record to publish one.

Expose source-loss diagnostics through a versioned/additive output contract,
including TUI/web guidance and Prometheus counters; no claim of complete kernel
coverage is acceptable without them. Default previews remain bounded and escaped.
The device's readiness and cancellation behavior needs real Linux VM tests;
ordinary Docker containers are not evidence of host kernel access.

`dmesg` is a useful compatibility recipe and fixture producer, not a silent
fallback. Its raw display and `/dev/kmsg` record format differ; JSON output must
not be assumed to be NDJSON. Never use `--clear` or `--read-clear` in examples.
If a later `--format dmesg` is worthwhile, give it a separate verified grammar
and continuation policy. [dmesg manual](https://man7.org/linux/man-pages/man1/dmesg.1.html)

## Architecture and compatibility

| Owner | Planned change |
| --- | --- |
| `src/cli.rs` | Native-source and format variants, bounded filters, explicit conflict matrix |
| `src/input.rs`, `src/input/apple.rs` | Extract owned-child behavior with regression coverage; add journal adapter |
| New `src/input/journal.rs` | Command construction, scoped source label, native failure handling |
| New `src/parse/journal.rs`, `src/parse.rs` | Portable journal parser and authority rules |
| `src/engine.rs`, `src/runtime.rs` | Source selection/labels and lifecycle propagation without bypassing framing |
| `src/parse/diagnostics.rs`, `src/web_frame.rs`, `web/frame.mjs` | Journal evidence, compatible frame encoding/decoding, bounded hints |
| Later `src/input/kmsg.rs` | Record-oriented device reads and source-loss diagnostics |
| `tests/`, `fuzz/`, `browser-tests/` | Fixtures, lifecycle regression cases, bounded-resource and output compatibility checks |

The current parser evidence is a six-element positional array. Do not append a
seventh element and keep claiming the old frame shape. Keep the existing six
categories stable and encode journal/kmsg evidence as an optional named extension
with matching browser validation; older frames may omit it. Explicitly test the
recognized-sum invariants across old and new fields. If this cannot be additive,
version the web frame contract before shipping.

Keep CLI JSON schema 2, webhook schema 1, and existing metric semantics unchanged
for the initial journal-only increment, which needs no new headless counters.
Kernel-loss metrics are a subsequent explicit output-contract change. Keep
allocation bounded by configured record size; no full per-record JSON tree and
no unbounded diagnostic queue. The static Linux packages must not gain a runtime
libsystemd dependency.

## Delivery sequence and acceptance

Each row is an independently reviewable increment. Split implementation and its
necessary tests into focused commits; commit a passing increment before moving
on. Do not put these features into the frozen 0.3.0 candidate.

| Step | Deliverable | Acceptance |
| --- | --- | --- |
| L1 | Portable journal format, mapping, malformed/unsupported diagnostics | Exact fixture accounting and all priority/shape cases pass on Mac and Linux |
| L2 | Extract common owned-child mechanics | Apple fixtures, real Mac capture, cancellation, stderr flood, and descriptor-flag tests remain unchanged |
| L3 | Native system journal and units | Real Linux service events reach text/TUI/web; default attachment excludes backlog; missing command and denied access are explicit |
| L4 | User journal and kernel-via-journal | Filter scope verified with independent units/users and kernel fixture; invalid combinations rejected before capture |
| L5 | Dashboard evidence and installed recipes | Correct hints, freeze/resume, final failure state, bounded previews, no unreadable small-terminal layout |
| L6 | Historical native query, if needed | Explicit finite/follow contracts; no silent replay-rate claims; cursor persistence remains separate |
| L7 | `/dev/kmsg` parser and native adapter | Record boundaries, sequence gaps, EPIPE, permissions, and cancellation verified on real Linux |
| L8 | Distribution and launch examples | Installed Linux x86-64/ARM64 binaries reproduce documented commands; Mac parser and Apple source remain green |

L1–L5 and installed validation are the first Linux release slice. L6 and L7 are
follow-ups with independent acceptance, not reasons to postpone journald.

The parser matrix must include all eight priorities; missing/null/wrong-type and
repeated values; duplicate keys; binary/array `MESSAGE`; escaped newlines; huge
ignored integers; bad Unicode; ANSI/control injection; malformed and oversized
records; final unterminated input; arbitrary chunk boundaries; and file rotation.

The adapter matrix must include empty accessible journals, absent units, delayed
first output, denied system access with usable user access, executable absence,
unsupported arguments, high stderr volume, hung child, child exit 0/nonzero,
stdout close, stdin EOF independence, normal signals, web bind/open failure,
blocked output, and no child or file-descriptor leaks over repeated starts.
Use injected executable fixtures for deterministic tests; real systemd tests
remain separate and report their actual host/version and privileges.

Run Rust tests and clippy in default, CLI-only, and snapshot-notification builds;
parser/framing fuzzing; terminal freshness; browser recovery; package/static-link
checks; exact-count short loads; and paired throughput/RSS comparisons. Confirm
unknowns remain in log-error denominators and filtered sources are labeled.
Run any day-long soaks only as optional background validation. They never block
release, CI, or publishing.

## Release and documentation checklist

- Add `docs/LINUX-INPUTS.md` with installable commands, source scope, mappings,
  permissions, missing-systemd guidance, and finite/live distinctions.
- Add a systemd service recipe and an SSH pipe plus explicit loopback web-tunnel
  recipe for remote servers. No browser auto-open is needed on headless hosts.
- Explain why `/var/log/syslog` and `/var/log/messages` may lack usable severity,
  and show priority-preserving exports rather than keyword guessing.
- Explain FIFO consumption through a pipe and its writer/EOF behavior; do not
  expand named-file acceptance to devices or sockets as an incidental change.
- Record tested distribution/systemd versions, architectures, binary provenance,
  memory bounds, and limitations with the release candidate.
- Demonstrate a known service emitting info, warning, and error journal priorities,
  then show a quiet source and a deliberate capture failure as distinct states.

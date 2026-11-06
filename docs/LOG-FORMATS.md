# Log format recognition

These additions are on current source and are not in the published 0.2.0
packages. Build with `cargo build --locked`; use `./target/debug/tell-tail` in
place of `tell-tail` below to avoid an older installed binary.

```sh
my-app 2>&1 | tell-tail --output web --open
tell-tail app.log --from start --output web --open
```

Automatic recognition runs independently on each physical record. It does not
lock the stream to the first format it encounters. Severity words inside a
message never determine severity. Unknown records still count toward throughput
and ratio denominators. Coverage is recognition, not application health.

## JSON fields

Without `--level-field`, JSON objects recognize these exact top-level keys:
`level`, `severity`, `levelname`, `Level`, `log.level`, `severityText`, and
`SeverityText`. Values are case-insensitive severity names; `Information` and
`Informational` map to info in addition to the existing warning/critical/panic
aliases. Field names are case-sensitive.

Multiple candidate fields must agree after normalization. Conflicting values,
or a recognized value alongside an unsupported value, produce unknown. Duplicate
candidate keys are malformed, even when their values agree. The entire JSON
record is validated, including ignored properties and Unicode escapes. Malformed
JSON does not fall back to text.

An explicit flag selects exactly one literal top-level key and disables field
inference, including when the selected key is `level`:

```sh
tell-tail app.log --level-field severity
tell-tail app.log --level-field log.level
```

The second example selects the literal dotted key, not a nested object path.
Absent or unsupported selected values remain unknown. Auto mode does not invent
severity for records with no level, or interpret bare numbers as severity.

The compatibility corpus is described in [the fixture guide](../tests/fixtures/README.md).
Its synthetic examples measure regression behavior, not real-world market coverage.

## Console text

Text accepts a leading severity (`WARN slow`, `[WARN] slow`, `ERROR:timeout`),
including Python's `WARNING:logger:message` layout. A preceding ISO-shaped
 timestamp can use `T` or a space between date and time, dot/comma fractional
seconds, and optional surrounding brackets. These timestamps are recognized as
header structure, not used for metric time or calendar-validated.

ANSI SGR color/style sequences in the first 512 input bytes are ignored for
classification using a fixed stack buffer. Other escape sequences and incomplete
color codes remain unrecognized. Raw input byte counts do not change; dashboard
previews keep their existing sanitization. Levels buried after arbitrary logger,
thread, or message prefixes are not searched for. Stack-trace continuation lines
still count individually and usually remain unknown.

## Key/value records (logfmt)

Auto mode selects logfmt when a record begins with an unquoted `key=` token.
Use `--format logfmt` to require it, or `--format text` to disable this detection.
The JSON field-name allowlist and explicit `--level-field` override apply here too.

```sh
tell-tail app.log --format logfmt
tell-tail app.log --format logfmt --level-field priority
```

For example, `time=2026-11-05T12:00:00Z level=WARN msg="upstream slow"` is warn.
`msg="level=error" level=info` is info. Keys use ASCII letters/digits and
`_ . - / @`; values are unquoted tokens or JSON-style quoted strings with escapes.
The entire record must consist of pairs. Invalid quoting, invalid UTF-8, invalid
Unicode escapes, duplicate severity keys, and trailing non-pair text are malformed
and counted once as unknown plus a parse failure. Numeric values without an
explicit mapping remain unknown. This is a defined logfmt subset, not a parser
for every application's loosely formatted key/value message.

## Nested JSON fields

`--level-path` selects an object path using JSON Pointer syntax:

```sh
tell-tail app.log --level-path /log/level
```

This reads `{"log":{"level":"error"}}`. It is separate from the literal
`--level-field log.level` selector. Escape a slash in a key as `~1` and a tilde
as `~0`; `/` selects an empty top-level key. Paths support at most 16 object keys
and 1024 UTF-8 bytes. Array indexing and recursive searches are not supported.
Missing paths or non-object intermediate values remain unknown. Duplicate keys
along the selected path are malformed. All other JSON still undergoes validation.

A path requires auto/JSON mode and selects JSON parsing exclusively. It conflicts
with `--level-field`, the built-in demo, and Apple unified format. Traversal uses
bounded streaming visitors and borrowed raw subobjects, not a full JSON tree.

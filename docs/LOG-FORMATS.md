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

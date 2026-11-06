# Classification corpus

These fixtures are synthetic, checked-in examples of the supported text grammar
and structured log shapes. They contain no collected application logs. Each case
specifies the expected severity of every physical record, parse-failure count,
and oversized count. Hex input represents invalid UTF-8 without relying on editor
encoding behavior.

ANSI SGR prefixes and logfmt are supported, with negative cases for misleading
message text. Nested fields and numeric levels require explicit selectors/profiles.
New format support needs an explicit contract change and updated expectations.

`tests/properties.rs` complements this fixed corpus with generated inputs. Proptest
persists failure seeds when it finds a counterexample; commit the resulting seed
and a minimized regression with its fix. Reproduce a reported run with
`PROPTEST_RNG_SEED=<reported seed> cargo test --test properties`. Input sizes and
case counts are capped for pull-request runs.

## Common-style compatibility baseline

`log-styles.json` adds synthetic examples based on documented conventions from
[Python logging](https://docs.python.org/3/library/logging.html),
[Go slog](https://pkg.go.dev/log/slog),
[Pino](https://github.com/pinojs/pino/blob/main/docs/api.md),
[Spring/ECS](https://docs.spring.io/spring-boot/reference/features/logging.html),
[Serilog compact JSON](https://github.com/serilog/serilog-formatting-compact), and
[OpenTelemetry](https://opentelemetry.io/docs/specs/otel/logs/data-model/).
They are hand-authored shape examples, not captured logs or a held-out dataset.
`expected` records current behavior; `target` records intended behavior, including
intentional unknowns. `planned_flags` are not active until the associated feature
lands. Promote each case and its flags in the same commit as implementation.

Run `cargo test --test log_styles -- --nocapture` for the baseline. The report is
an equally weighted fixture result, not a claim about ecosystem market share.
All cases run across several byte boundaries. Keep ambiguous numeric values,
conflicting fields, message keywords, and no-severity records as negative cases.

use clap::Parser;
use serde::Deserialize;
use std::time::Duration;
use tell_tail::{cli::Args, engine::Engine, parse::Level, stats::State};

#[derive(Deserialize)]
struct Case {
    name: String,
    input: String,
    flags: Vec<String>,
    expected: String,
    target: String,
}

#[test]
fn documented_log_styles_have_explicit_expectations_across_read_boundaries() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/log-styles.json")).unwrap();
    let mut covered = 0;
    for case in &cases {
        let args = Args::parse_from(
            std::iter::once("tell-tail").chain(case.flags.iter().map(String::as_str)),
        );
        args.validate().unwrap();
        for chunk in [1, 7, 65536] {
            let mut engine = Engine::new(&args);
            let input = format!("{}\n", case.input);
            for bytes in input.as_bytes().chunks(chunk) {
                engine.feed(bytes, Duration::ZERO);
            }
            engine.finish(Duration::ZERO);
            let snapshot = engine.snapshot(Duration::from_secs(1), State::Complete, None);
            assert_eq!(snapshot.totals.records, 1, "{}", case.name);
            assert_eq!(snapshot.totals.parse_failures, 0, "{}", case.name);
            let level = Level::from_bytes(case.expected.as_bytes());
            assert_eq!(
                snapshot.totals.levels.get(level),
                1,
                "{} chunk {chunk}",
                case.name
            );
        }
        covered += usize::from(case.expected == case.target);
    }
    eprintln!(
        "Log-style expectations: {covered}/{} at intended behavior (includes intentional unknowns; not market coverage)",
        cases.len()
    );
}

#[test]
fn automatic_json_fields_agree_or_remain_unknown_and_explicit_fields_win() {
    use tell_tail::parse::Classifier;
    let auto = Classifier::from_args(&Args::parse_from(["tell-tail"]));
    for (line, level, malformed) in [
        (
            r#"{"severity":"warning","level":"WARN"}"#,
            Level::Warn,
            false,
        ),
        (
            r#"{"severity":"error","level":"info"}"#,
            Level::Unknown,
            false,
        ),
        (
            r#"{"level":"info","severity":"error","Level":"Information"}"#,
            Level::Unknown,
            false,
        ),
        (
            r#"{"severity":null,"level":"error"}"#,
            Level::Unknown,
            false,
        ),
        (
            r#"{"severity":"warn","severity":"warn"}"#,
            Level::Unknown,
            true,
        ),
        (
            r#"{"severity":"error","message":"\uD800"}"#,
            Level::Unknown,
            true,
        ),
    ] {
        let result = auto.classify(line.as_bytes());
        assert_eq!(
            (result.level, result.parse_failure),
            (level, malformed),
            "{line}"
        );
    }
    let explicit =
        Classifier::from_args(&Args::parse_from(["tell-tail", "--level-field", "level"]));
    assert_eq!(
        explicit.classify(br#"{"severity":"error"}"#).level,
        Level::Unknown
    );
    assert_eq!(
        explicit
            .classify(br#"{"level":"info","severity":"error"}"#)
            .level,
        Level::Info
    );
}

#[test]
fn nested_paths_are_literal_bounded_and_validate_the_complete_record() {
    use tell_tail::parse::Classifier;
    let args = Args::parse_from(["tell-tail", "--level-path", "/log/level"]);
    args.validate().unwrap();
    let c = Classifier::from_args(&args);
    for (line, expected, malformed) in [
        (
            r#"{"log":{"level":"error"},"level":"info"}"#,
            Level::Error,
            false,
        ),
        (r#"{"log.level":"error"}"#, Level::Unknown, false),
        (r#"{"log":[{"level":"error"}]}"#, Level::Unknown, false),
        (
            r#"{"log":{"level":"warn","level":"warn"}}"#,
            Level::Unknown,
            true,
        ),
        (
            r#"{"log":{"level":"error"},"other":"\uD800"}"#,
            Level::Unknown,
            true,
        ),
        (
            r#"{"log":{"level":"error"},"other":}"#,
            Level::Unknown,
            true,
        ),
    ] {
        let result = c.classify(line.as_bytes());
        assert_eq!(
            (result.level, result.parse_failure),
            (expected, malformed),
            "{line}"
        );
    }
    let escaped = Classifier::from_args(&Args::parse_from([
        "tell-tail",
        "--level-path",
        "/a~1b/~0level",
    ]));
    assert_eq!(
        escaped.classify(br#"{"a/b":{"~level":"warn"}}"#).level,
        Level::Warn
    );
    for path in [
        "",
        "log.level",
        "/~2bad",
        "/~",
        &"/x".repeat(17),
        &format!("/{}", "x".repeat(1024)),
    ] {
        assert!(
            Args::parse_from(["tell-tail", "--level-path", path])
                .validate()
                .is_err()
        );
    }
    assert!(
        Args::try_parse_from([
            "tell-tail",
            "--level-path",
            "/log/level",
            "--level-field",
            "level"
        ])
        .is_err()
    );
}

#[test]
fn numeric_profiles_are_explicit_and_keep_distinct_mappings() {
    use tell_tail::parse::Classifier;
    for (profile, field, numbers, levels) in [
        (
            "pino",
            "level",
            vec![0, 10, 20, 30, 40, 50, 60, 70],
            vec![
                Level::Unknown,
                Level::Trace,
                Level::Debug,
                Level::Info,
                Level::Warn,
                Level::Error,
                Level::Fatal,
                Level::Unknown,
            ],
        ),
        (
            "python",
            "levelno",
            vec![0, 10, 20, 30, 40, 50, 60],
            vec![
                Level::Unknown,
                Level::Debug,
                Level::Info,
                Level::Warn,
                Level::Error,
                Level::Fatal,
                Level::Unknown,
            ],
        ),
        (
            "otel",
            "severityNumber",
            vec![0, 1, 4, 5, 8, 9, 12, 13, 16, 17, 20, 21, 24, 25],
            vec![
                Level::Unknown,
                Level::Trace,
                Level::Trace,
                Level::Debug,
                Level::Debug,
                Level::Info,
                Level::Info,
                Level::Warn,
                Level::Warn,
                Level::Error,
                Level::Error,
                Level::Fatal,
                Level::Fatal,
                Level::Unknown,
            ],
        ),
    ] {
        let args = Args::parse_from(["tell-tail", "--profile", profile]);
        args.validate().unwrap();
        let c = Classifier::from_args(&args);
        for (n, expected) in numbers.into_iter().zip(levels) {
            assert_eq!(
                c.classify(format!(r#"{{"{field}":{n}}}"#).as_bytes()).level,
                expected,
                "{profile} {n}"
            );
        }
        for value in ["-1", "30.0", "1e1", "256", "null", "true", "\"30\""] {
            assert_eq!(
                c.classify(format!(r#"{{"{field}":{value}}}"#).as_bytes())
                    .level,
                Level::Unknown
            );
        }
    }
    let overridden = Classifier::from_args(&Args::parse_from([
        "tell-tail",
        "--profile",
        "pino",
        "--level-path",
        "/event/priority",
    ]));
    assert_eq!(
        overridden
            .classify(br#"{"level":10,"event":{"priority":50}}"#)
            .level,
        Level::Error
    );
    let serilog = Classifier::from_args(&Args::parse_from(["tell-tail", "--profile", "serilog"]));
    for (line, expected) in [
        (r#"{"@l":"Verbose"}"#, Level::Trace),
        (r#"{"@t":"2026-11-05T12:00:00Z","@m":"ready"}"#, Level::Info),
        (r#"{"@t":"not a timestamp","@m":"ready"}"#, Level::Unknown),
        (r#"{"@m":"ready"}"#, Level::Unknown),
        (
            r#"{"@t":"2026-11-05T12:00:00Z","@m":"ready","@l":null}"#,
            Level::Unknown,
        ),
    ] {
        assert_eq!(serilog.classify(line.as_bytes()).level, expected, "{line}");
    }
    assert!(Args::try_parse_from(["tell-tail", "--demo", "--profile", "pino"]).is_err());
    for format in ["text", "logfmt", "apple-unified"] {
        assert!(
            Args::parse_from(["tell-tail", "--profile", "pino", "--format", format])
                .validate()
                .is_err()
        );
    }
}

#[test]
fn parsing_evidence_distinguishes_ambiguity_from_malformed_records() {
    use tell_tail::parse::{Classifier, ParseIssue, ParsingDiagnostics};
    let auto = Classifier::from_args(&Args::parse_from(["tell-tail"]));
    let mut diagnostics = ParsingDiagnostics::default();
    for (line, issue) in [
        (r#"{"level":50}"#, ParseIssue::Numeric),
        (r#"{"levelno":30}"#, ParseIssue::Numeric),
        (
            r#"{"level":"info","severity":"error"}"#,
            ParseIssue::Conflicting,
        ),
        ("level=error severity=warn", ParseIssue::Conflicting),
        (r#"{"level":50,"bad":}"#, ParseIssue::None),
        ("INFO ready", ParseIssue::None),
        (r#"{"severity":"warn"}"#, ParseIssue::None),
        ("level=error msg=timeout", ParseIssue::None),
    ] {
        let assessment = auto.assess_record(line.as_bytes()).unwrap();
        assert_eq!(assessment.issue, issue, "{line}");
        diagnostics.record(assessment);
    }
    assert_eq!(diagnostics.recognized, [1, 1, 1, 0, 0, 0]);
    assert_eq!(diagnostics.numeric, 2);
    assert_eq!(diagnostics.conflicting, 2);
    assert!(diagnostics.hint().contains("--level-field"));
}

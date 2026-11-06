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

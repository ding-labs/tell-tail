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

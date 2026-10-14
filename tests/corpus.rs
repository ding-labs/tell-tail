use clap::Parser;
use serde::Deserialize;
use std::time::Duration;
use tell_tail::{cli::Args, engine::Engine, parse::Level, stats::State};

#[derive(Deserialize)]
struct Case {
    name: String,
    input: Option<String>,
    hex: Option<String>,
    format: Option<String>,
    level_field: Option<String>,
    max_record_bytes: Option<u32>,
    levels: Vec<String>,
    parse_failures: u64,
    oversized_records: u64,
}
#[test]
fn corpus_counts_are_independent_of_read_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/classification.json")).unwrap();
    for case in cases {
        let bytes = match (case.input, case.hex) {
            (Some(s), None) => s.into_bytes(),
            (None, Some(hex)) => (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect(),
            _ => panic!("fixture {} must select exactly one input", case.name),
        };
        let mut args = Args::parse_from([
            "tell-tail",
            "--format",
            case.format.as_deref().unwrap_or("auto"),
        ]);
        args.level_field = case.level_field.unwrap_or_else(|| "level".into());
        args.max_record_bytes = case.max_record_bytes.unwrap_or(1024);
        for chunk in [1, 2, 3, 7, 64, 65536] {
            let mut engine = Engine::new(&args);
            for part in bytes.chunks(chunk) {
                engine.feed(part, Duration::ZERO);
            }
            engine.finish(Duration::ZERO);
            let report = engine.snapshot(Duration::from_secs(1), State::Complete, Some(0));
            assert_eq!(
                report.totals.records,
                case.levels.len() as u64,
                "{} chunk {chunk}",
                case.name
            );
            for level in Level::ALL {
                assert_eq!(
                    report.totals.levels.get(level),
                    case.levels.iter().filter(|s| *s == level.name()).count() as u64,
                    "{} {} chunk {chunk}",
                    case.name,
                    level.name()
                );
            }
            assert_eq!(
                report.totals.parse_failures, case.parse_failures,
                "{} chunk {chunk}",
                case.name
            );
            assert_eq!(
                report.totals.oversized_records, case.oversized_records,
                "{} chunk {chunk}",
                case.name
            );
            assert_eq!(report.bytes_read, bytes.len() as u64);
            assert_eq!(report.pending_bytes, 0);
        }
    }
}

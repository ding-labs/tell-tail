use clap::Parser;
use std::time::Duration;
use std::{io::Write, process::Command};
use tell_tail::{
    cli::Args,
    engine::Engine,
    parse::{Classification, Classifier, Level},
    stats::State,
};

#[test]
fn apple_types_are_mapped_only_in_the_explicit_format() {
    let args = Args::parse_from(["tell-tail", "--format", "apple-unified"]);
    assert!(args.validate().is_ok());
    let classifier = Classifier::new(args.format, args.level_field);
    for (value, level) in [
        ("Default", Level::Info),
        ("Info", Level::Info),
        ("Debug", Level::Debug),
        ("Error", Level::Error),
        ("Fault", Level::Fatal),
        ("Other", Level::Unknown),
    ] {
        let line = format!(r#"{{"messageType":"{value}","level":"fatal"}}"#);
        assert_eq!(
            classifier.classify_record(line.as_bytes()),
            Some(Classification {
                level,
                parse_failure: false
            })
        );
    }
    for line in [
        br#"{"messageType":"F\u0061ult"}"#.as_slice(),
        br#"{"messageType":"fault"}"#,
    ] {
        assert_eq!(
            classifier.classify_record(line).unwrap().level,
            Level::Fatal
        );
    }
    let generic = Classifier::new(tell_tail::cli::Format::Json, "messageType".into());
    assert_eq!(
        generic.classify(br#"{"messageType":"Default"}"#).level,
        Level::Unknown
    );
    assert_eq!(
        generic.classify(br#"{"messageType":"Fault"}"#).level,
        Level::Unknown
    );
}

#[test]
fn metadata_is_skipped_but_malformed_records_remain_visible() {
    let classifier = Classifier::new(tell_tail::cli::Format::AppleUnified, "level".into());
    for line in [
        "Filtering the log data using type == log",
        "",
        "{}",
        r#"{"messageType":null}"#,
        r#"{"nested":{"messageType":"Error"}}"#,
    ] {
        assert_eq!(classifier.classify_record(line.as_bytes()), None, "{line}");
    }
    for line in [
        br#"{"messageType":"Error",}"#.as_slice(),
        br#"{"messageType":null,"messageType":"Error"}"#,
        br#"{"messageType":"Info","message":"\uD800"}"#,
        b"{\"messageType\":\"Info\",\"message\":\"\xff\"}",
    ] {
        assert_eq!(
            classifier.classify_record(line),
            Some(Classification {
                level: Level::Unknown,
                parse_failure: true
            })
        );
    }
    let large_number = br#"{"messageType":"Error","irrelevant":1e400}"#;
    assert_eq!(
        classifier.classify_record(large_number).unwrap().level,
        Level::Error
    );
    assert!(
        !classifier
            .classify_record(large_number)
            .unwrap()
            .parse_failure
    );
}

#[test]
fn apple_capture_counts_and_skipped_bytes_are_chunk_independent() {
    let args = Args::parse_from(["tell-tail", "--format", "apple-unified"]);
    let metadata = [
        "Filtering the log data using type == log\r\n",
        "{\"messageType\":null}\n",
        "{}\n",
        "trailer",
    ];
    let mut input = metadata[..3].concat();
    input.push_str("{\"messageType\":\"Default\"}\n{\"messageType\":\"Info\"}\r\n{\"messageType\":\"Debug\"}\n{\"messageType\":\"Error\"}\n{\"messageType\":\"Fault\"}\n{\"messageType\":42}\n{bad}\n");
    input.push_str(metadata[3]);
    for chunk in [1, 2, 7, 64, 65536] {
        let mut engine = Engine::new(&args);
        for part in input.as_bytes().chunks(chunk) {
            engine.feed(part, Duration::ZERO);
        }
        engine.finish(Duration::ZERO);
        let frame = engine.snapshot(Duration::ZERO, State::Complete, None);
        assert_eq!(frame.totals.records, 7);
        assert_eq!(frame.totals.levels.get(Level::Info), 2);
        for level in [Level::Debug, Level::Error, Level::Fatal] {
            assert_eq!(frame.totals.levels.get(level), 1);
        }
        assert_eq!(frame.totals.levels.get(Level::Unknown), 2);
        assert_eq!(frame.totals.parse_failures, 1);
        assert_eq!(frame.bytes_read, input.len() as u64);
        assert_eq!(
            frame.skipped_bytes,
            metadata.iter().map(|s| s.len() as u64).sum::<u64>()
        );
        assert_eq!(frame.pending_bytes, 0);
    }
}

#[test]
fn oversized_apple_records_keep_the_existing_unknown_diagnostic() {
    let args = Args::parse_from([
        "tell-tail",
        "--format",
        "apple-unified",
        "--max-record-bytes",
        "8",
    ]);
    let mut engine = Engine::new(&args);
    engine.feed(b"{\"messageType\":\"Error\"}\n{}\n", Duration::ZERO);
    engine.finish(Duration::ZERO);
    let frame = engine.snapshot(Duration::ZERO, State::Complete, None);
    assert_eq!(frame.totals.records, 1);
    assert_eq!(frame.totals.oversized_records, 1);
    assert_eq!(frame.totals.levels.get(Level::Unknown), 1);
    assert_eq!(frame.skipped_bytes, 3);
}

#[test]
fn saved_apple_ndjson_runs_through_the_existing_file_cli() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(
        b"Filtering the log data\n{\"messageType\":\"Default\"}\n{\"messageType\":\"Fault\"}\n",
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg(file.path())
        .args(["--once", "--format", "apple-unified", "--output", "json"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let frame: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(frame["totals"]["records"], 2);
    assert_eq!(frame["totals"]["levels"]["info"], 1);
    assert_eq!(frame["totals"]["levels"]["fatal"], 1);
    assert_eq!(frame["totals"]["parse_failures"], 0);
    assert_eq!(frame["skipped_bytes"], b"Filtering the log data\n".len());
    assert_eq!(frame["error_ratio"], 0.5);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn live_apple_capture_is_rejected_clearly_on_other_platforms() {
    let output = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg("--apple-unified")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires macOS"));
}

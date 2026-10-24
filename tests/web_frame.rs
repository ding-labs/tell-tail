use serde_json::Value;
use std::time::Duration;
use tell_tail::{
    stats::{Counts, Levels, Snapshot, State, Stats},
    web_frame::{EncodeError, MAX_FRAME_BYTES, WebFrame},
};

fn snapshot(at: Duration, state: State) -> Snapshot {
    let mut snapshot = Stats::new(60, "stdin".into()).snapshot(at, state, 0, None);
    snapshot.session_id = "fixture".into();
    snapshot.observed_at_unix_seconds = 0.0;
    snapshot
}

fn json(sequence: u64, snapshot: &Snapshot) -> Value {
    serde_json::from_str(WebFrame::encode(sequence, snapshot).unwrap().json()).unwrap()
}

#[test]
fn empty_web_schema_matches_reviewed_fixture_without_changing_cli_schema() {
    let snapshot = snapshot(Duration::ZERO, State::Complete);
    let frame = WebFrame::encode(0, &snapshot).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/output/empty-web-v1.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(frame.json()).unwrap(),
        expected
    );
    assert_eq!(frame.sequence(), 0);
    assert_eq!(frame.event_id(), "fixture:0");
    assert!(!frame.json().contains(['\r', '\n', '\0']));
    let cli = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(cli["schema_version"], 2);
    assert_eq!(cli["totals"]["records"], 0);
}

#[test]
fn every_counter_preserves_u64_precision_including_zero_and_above_js_safe_integer() {
    for n in [0, 1, (1 << 53) - 1, 1 << 53, (1 << 53) + 1, u64::MAX] {
        let mut snapshot = snapshot(Duration::from_secs(60), State::Following);
        snapshot.totals = Counts {
            records: n,
            levels: Levels([n; 7]),
            parse_failures: n,
            oversized_records: n,
        };
        snapshot.window.counts = snapshot.totals;
        snapshot.bytes_read = n;
        snapshot.pending_bytes = n;
        snapshot.skipped_bytes = n;
        snapshot.discarded_partial_records = n;
        snapshot.backlog_bytes = Some(n);
        snapshot.window.bytes_read = n;
        snapshot.history.fill(n);
        let frame = json(n, &snapshot);
        assert_eq!(frame["sequence"], n.to_string());
        let metrics = &frame["metrics"];
        for key in [
            "bytes_read",
            "pending_bytes",
            "skipped_bytes",
            "discarded_partial_records",
            "backlog_bytes",
        ] {
            assert_eq!(metrics[key], n.to_string(), "{key}");
        }
        for counts in [&metrics["totals"], &metrics["window"]["counts"]] {
            for key in ["records", "parse_failures", "oversized_records"] {
                assert_eq!(counts[key], n.to_string(), "{key}");
            }
            for level in [
                "trace", "debug", "info", "warn", "error", "fatal", "unknown",
            ] {
                assert_eq!(counts["levels"][level], n.to_string(), "{level}");
            }
        }
        assert_eq!(metrics["window"]["bytes_read"], n.to_string());
        assert_eq!(
            metrics["history"],
            serde_json::json!(vec![n.to_string(); 60])
        );
        assert_eq!(metrics["window"]["configured_seconds"], 60);
        for value in [
            &metrics["observed_at_unix_seconds"],
            &metrics["elapsed_seconds"],
            &metrics["window"]["resolution_seconds"],
            &metrics["window"]["observed_seconds"],
            &metrics["window"]["records_per_second"],
            &metrics["window"]["bytes_per_second"],
        ] {
            assert!(value.is_number());
        }
    }
}

#[test]
fn all_live_and_final_states_keep_the_same_shape_and_nullable_ratios() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/output/empty-web-v1.json")).unwrap();
    let keys: Vec<_> = fixture["metrics"].as_object().unwrap().keys().collect();
    for (state, name, kind) in [
        (State::Reading, "reading", "snapshot"),
        (State::Following, "following", "snapshot"),
        (State::Waiting, "waiting", "snapshot"),
        (State::Complete, "complete", "final"),
        (State::Stopped, "stopped", "final"),
        (State::Failed, "failed", "final"),
    ] {
        let mut snapshot = snapshot(Duration::from_millis(500), state);
        let empty = json(7, &snapshot);
        assert_eq!(empty["metrics"]["state"], name);
        assert_eq!(empty["metrics"]["kind"], kind);
        for ratios in [&empty["metrics"], &empty["metrics"]["window"]] {
            assert!(ratios["warning_ratio"].is_null());
            assert!(ratios["error_ratio"].is_null());
        }
        snapshot.warning_ratio = Some(0.0);
        snapshot.error_ratio = Some(1.0);
        snapshot.window.warning_ratio = Some(0.25);
        snapshot.window.error_ratio = Some(0.5);
        let nonempty = json(8, &snapshot);
        assert_eq!(nonempty["metrics"]["warning_ratio"], 0.0);
        assert_eq!(nonempty["metrics"]["error_ratio"], 1.0);
        assert_eq!(nonempty["metrics"]["window"]["warning_ratio"], 0.25);
        assert_eq!(nonempty["metrics"]["window"]["error_ratio"], 0.5);
        for value in [&empty["metrics"], &nonempty["metrics"]] {
            assert_eq!(value.as_object().unwrap().keys().collect::<Vec<_>>(), keys);
        }
    }
}

#[test]
fn history_contains_completed_seconds_oldest_first_and_replaces_each_frame() {
    use clap::Parser;
    use tell_tail::{cli::Args, engine::Engine};
    let mut engine = Engine::new(&Args::parse_from(["tell-tail"]));
    for second in 0..=65 {
        engine.feed(
            &b"INFO x\n".repeat(second as usize + 1),
            Duration::from_secs(second),
        );
        for millis in [0, 100, 999] {
            let elapsed = Duration::from_secs(second) + Duration::from_millis(millis);
            let snapshot = engine.snapshot(elapsed, State::Following, None);
            let value = json(second * 10 + millis / 100, &snapshot);
            let expected: Vec<String> = (second.saturating_sub(60)..second)
                .map(|s| (s + 1).to_string())
                .collect();
            assert_eq!(value["metrics"]["history"], serde_json::json!(expected));
            assert_eq!(value["metrics"]["elapsed_seconds"], elapsed.as_secs_f64());
        }
    }
}

#[test]
fn dashboard_previews_are_optional_bounded_and_preserve_exact_record_numbers() {
    use clap::Parser;
    use tell_tail::{cli::Args, engine::Engine, recent::MAX_RECENT_ERRORS};
    let mut engine = Engine::new(&Args::parse_from(["tell-tail", "--output", "tui"]));
    for _ in 0..25 {
        engine.feed(b"ERROR example\n", Duration::ZERO);
    }
    engine.feed(b"FATAL last\n", Duration::from_secs(1));
    let mut snapshot = engine.snapshot(Duration::from_secs(1), State::Following, None);
    snapshot.recent_errors[0].record = u64::MAX;
    let value = json(1, &snapshot);
    assert_eq!(
        value["recent_errors"].as_array().unwrap().len(),
        MAX_RECENT_ERRORS
    );
    assert_eq!(value["recent_errors"][0]["record"], u64::MAX.to_string());
    assert_eq!(value["recent_errors"][0]["level"], "fatal");
    assert_eq!(value["recent_errors"][0]["preview"], "FATAL last");
    assert!(
        serde_json::to_value(&snapshot)
            .unwrap()
            .get("recent_errors")
            .is_none()
    );
    snapshot.recent_errors[0].preview = "é".repeat(257).into();
    assert!(matches!(
        WebFrame::encode(1, &snapshot),
        Err(EncodeError::InvalidRecentErrors)
    ));
    snapshot.recent_errors.clear();
    assert!(json(2, &snapshot).get("recent_errors").is_none());
}

#[test]
fn size_limit_counts_serialized_utf8_and_escaping_and_accepts_exact_boundary() {
    let mut snapshot = snapshot(Duration::ZERO, State::Reading);
    snapshot.source.clear();
    let overhead = WebFrame::encode(0, &snapshot).unwrap().json().len();
    // Two UTF-8 bytes + six JSON escape bytes + two escaped-quote bytes.
    snapshot.source = "é\0\"".repeat((MAX_FRAME_BYTES - overhead) / 10);
    let actual = WebFrame::encode(0, &snapshot).unwrap().json().len();
    snapshot
        .source
        .push_str(&"x".repeat(MAX_FRAME_BYTES - actual));
    let frame = WebFrame::encode(0, &snapshot).unwrap();
    assert_eq!(frame.json().len(), MAX_FRAME_BYTES);
    assert_eq!(
        serde_json::from_str::<Value>(frame.json()).unwrap()["metrics"]["source"],
        snapshot.source
    );
    snapshot.source.push('x');
    assert!(matches!(
        WebFrame::encode(0, &snapshot),
        Err(EncodeError::TooLarge)
    ));
    snapshot.source = "\n".repeat(MAX_FRAME_BYTES * 2);
    let error = WebFrame::encode(0, &snapshot).unwrap_err();
    assert!(matches!(error, EncodeError::TooLarge));
    assert!(error.to_string().len() < 100);
}

#[test]
fn invalid_data_fails_instead_of_becoming_null_or_injecting_sse_lines() {
    let mut snapshot = snapshot(Duration::ZERO, State::Reading);
    for id in ["", "x\ny", "x\ry", "x\0y"] {
        snapshot.session_id = id.into();
        assert!(matches!(
            WebFrame::encode(0, &snapshot),
            Err(EncodeError::InvalidSessionId)
        ));
    }
    snapshot.session_id = "fixture".into();
    snapshot.source = "x\ndata: bad\r\0</script>é".into();
    let frame = WebFrame::encode(0, &snapshot).unwrap();
    assert!(!frame.json().contains(['\r', '\n', '\0']));
    assert_eq!(json(0, &snapshot)["metrics"]["source"], snapshot.source);
    snapshot.history = vec![0; 61];
    assert!(matches!(
        WebFrame::encode(0, &snapshot),
        Err(EncodeError::InvalidHistory)
    ));
    snapshot.history.clear();
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        snapshot.elapsed_seconds = number;
        assert!(matches!(
            WebFrame::encode(0, &snapshot),
            Err(EncodeError::Serialization(_))
        ));
        snapshot.elapsed_seconds = 0.0;
        snapshot.window.error_ratio = Some(number);
        assert!(matches!(
            WebFrame::encode(0, &snapshot),
            Err(EncodeError::Serialization(_))
        ));
        snapshot.window.error_ratio = None;
    }
}

use std::{
    process::{Command, Stdio},
    time::Duration,
};
use tell_tail::{
    cli::Output,
    output,
    stats::{State, Stats},
};

#[test]
fn empty_schema_and_metrics_match_reviewed_fixtures() {
    let mut snapshot =
        Stats::new(60, "stdin".into()).snapshot(Duration::ZERO, State::Complete, 0, None);
    snapshot.session_id = "fixture".into();
    snapshot.observed_at_unix_seconds = 0.0;
    let actual: serde_json::Value =
        serde_json::from_str(&output::render(&snapshot, Output::Json).unwrap()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/output/empty-v2.json")).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        output::render(&snapshot, Output::Prometheus).unwrap(),
        include_str!("fixtures/output/empty.prom")
    );
    let text = output::render(&snapshot, Output::Text).unwrap();
    assert!(text.contains("warn=— error=—"));
    assert!(text.contains("unknown=0 parse_failures=0"));
    snapshot.backlog_bytes = Some(17);
    for (state, running, source_up) in [
        (State::Following, 1, 1),
        (State::Waiting, 1, 0),
        (State::Failed, 0, 0),
    ] {
        snapshot.state = state;
        let metrics = output::render(&snapshot, Output::Prometheus).unwrap();
        assert!(
            metrics.contains("# TYPE tell_tail_backlog_bytes gauge\ntell_tail_backlog_bytes 17\n")
        );
        assert!(metrics.contains(&format!("tell_tail_running {running}\n")));
        assert!(metrics.contains(&format!("tell_tail_source_up {source_up}\n")));
    }
}

#[test]
fn help_and_argument_errors_have_stable_exit_contracts() {
    let binary = env!("CARGO_BIN_EXE_tell-tail");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    let help = String::from_utf8(help.stdout).unwrap();
    for flag in [
        "--once",
        "--from",
        "--format",
        "--level-field",
        "--window",
        "--output",
        "--metrics-file",
        "--max-record-bytes",
        "--interval-ms",
        "--alert-webhook",
        "--alert-error-percent",
        "--alert-message",
        "--alert-min-records",
        "--alert-cooldown",
    ] {
        assert!(help.contains(flag), "{flag}");
    }
    assert!(!help.contains("--__tell-tail-output-worker"));
    assert!(!help.contains("--__tell-tail-webhook-worker"));
    for args in [
        vec!["--window", "0"],
        vec!["--window", "3601"],
        vec!["--interval-ms", "99"],
        vec!["--interval-ms", "60001"],
        vec!["--max-record-bytes", "0"],
        vec!["--max-record-bytes", "16777217"],
        vec!["--unknown"],
        vec!["--once"],
        vec!["--level-field", ""],
        vec!["--output", "invalid"],
        vec!["--metrics-file", "unused.prom"],
        vec!["--alert-error-percent", "25"],
        vec!["--alert-webhook", "http://localhost"],
        vec!["--alert-message", "missing endpoint"],
        vec!["--alert-min-records", "1"],
        vec!["--alert-cooldown", "0"],
    ] {
        let output = Command::new(binary)
            .args(&args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

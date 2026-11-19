use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn replay(path: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .env_remove("HOME")
        .args(["mcp", "--trace-replay"])
        .arg(path)
        .args(["--output", "json", "--once"])
        .output()
        .unwrap()
}

#[test]
fn replay_is_finite_private_and_does_not_read_client_logs() {
    let output = replay(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tool-trace/lifecycle.ndjson"
    )));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE"));
    let rows: Vec<Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r["schema_version"] == 1));
    assert_eq!(rows[0]["call"]["status"], "running");
    assert_eq!(rows[2]["call"]["status"], "completed");
    assert_eq!(rows[2]["call"]["duration_ms"], 20);
    assert_eq!(rows[3]["call"]["status"], "failed");
    assert_eq!(rows[0]["call"]["id"], rows[3]["call"]["id"]);
    assert_eq!(rows[4]["snapshot"]["counts"]["distinct_calls"], 2);
    assert_eq!(rows[4]["snapshot"]["counts"]["duplicate_observations"], 1);
    assert_eq!(rows[4]["live_capture_verified"], false);
    assert_eq!(rows[0]["call"]["historical"], true);
}

#[test]
fn malformed_oversized_and_unterminated_records_do_not_leak_or_claim_timeouts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trace.ndjson");
    let start = include_str!("fixtures/tool-trace/lifecycle.ndjson")
        .lines()
        .next()
        .unwrap();
    fs::write(
        &path,
        format!(
            "{start}\nPRIVATE_MALFORMED\n{}\n{start}",
            "x".repeat(20_000)
        ),
    )
    .unwrap();
    let output = replay(&path);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("PRIVATE"));
    let rows: Vec<Value> = text
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let end = rows.last().unwrap();
    assert_eq!(end["invalid_records"], "2");
    assert_eq!(end["oversized_records"], "1");
    assert!(rows.iter().any(|r| r["call"]["status"] == "incomplete"));
    assert!(!rows.iter().any(|r| r["call"]["status"] == "failed"));
}

#[test]
fn replay_rejects_live_modes_and_non_regular_inputs() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!replay(dir.path()).status.success());
    for flag in ["--open", "--demo"] {
        let output = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["mcp", "--trace-replay", "unused", flag])
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

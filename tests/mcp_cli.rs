use std::{fs, process::Command};
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tell-tail"))
}
#[test]
fn mcp_once_reports_scoped_failures_without_private_fields() {
    let t = tempfile::tempdir().unwrap();
    fs::write(t.path().join("desktop.log"),"2026-11-12T00:00:00Z warning [AppServerConnection] mcp_server_startup_status_updated server=example status=failed failureReason=reauthenticationRequired error=\"PRIVATE_PAYLOAD\" threadId=PRIVATE_THREAD\n2026-11-12T00:00:01Z error [git] unrelated\n").unwrap();
    let out = bin()
        .args(["mcp", "--source", "codex-desktop", "--log-dir"])
        .arg(t.path())
        .args(["--once", "--output", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("PRIVATE"));
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let events: Vec<_> = rows.iter().filter(|r| r["kind"] == "event").collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"]["reason"], "reauthentication");
    let status = rows.last().unwrap();
    assert_eq!(status["running"], false);
    assert_eq!(status["counts"]["ignored"], 1);
}
#[test]
fn absent_logs_and_literal_mcp_filenames_remain_usable() {
    let t = tempfile::tempdir().unwrap();
    let out = bin()
        .args(["mcp", "--source", "claude-desktop", "--log-dir"])
        .arg(t.path().join("missing"))
        .args(["--once", "--output", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(row["roots"][0]["status"], "waiting for directory");
    fs::write(t.path().join("mcp"), "INFO example\n").unwrap();
    for args in [
        vec!["./mcp", "--once", "--output", "json"],
        vec!["--once", "--output", "json", "--", "mcp"],
    ] {
        let out = bin().current_dir(t.path()).args(args).output().unwrap();
        assert!(out.status.success());
        let row: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(row["totals"]["records"], 1);
    }
}
#[test]
fn invalid_mcp_options_fail_before_reading_logs() {
    for args in [
        vec!["mcp", "--open", "--output", "json"],
        vec!["mcp", "--log-dir", "."],
        vec!["mcp", "--once", "--open"],
    ] {
        assert!(!bin().args(args).output().unwrap().status.success());
    }
    #[cfg(not(feature = "web"))]
    {
        let out = bin().args(["mcp", "--open"]).output().unwrap();
        assert!(String::from_utf8_lossy(&out.stderr).contains("web feature"));
    }
}

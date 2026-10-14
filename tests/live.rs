#![cfg(unix)]
use std::{
    fs::File,
    io::{Read, Write},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn stop(child: &mut Child) {
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(143));
            break;
        }
        if began.elapsed() > Duration::from_secs(3) {
            child.kill().unwrap();
            panic!("shutdown timed out");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(began.elapsed() < Duration::from_secs(1));
}
#[test]
fn idle_stdin_is_interruptible_and_reports_pending_bytes() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .args(["--output", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"WARN complete\nINFO partial")
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    stop(&mut child);
    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    let report: serde_json::Value = serde_json::from_str(output.lines().last().unwrap()).unwrap();
    assert_eq!(report["state"], "stopped");
    assert_eq!(report["totals"]["records"], 1);
    assert_eq!(report["pending_bytes"], 12);
}
#[test]
fn live_append_rotation_and_idle_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(&path, b"INFO old\n").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg(&path)
        .args([
            "--from",
            "start",
            "--output",
            "json",
            "--interval-ms",
            "100",
            "--window",
            "1",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    File::options()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"WARN appended\nunfinished")
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    std::fs::rename(&path, dir.path().join("old.log")).unwrap();
    std::fs::write(&path, b"ERROR rotated\n").unwrap();
    thread::sleep(Duration::from_millis(1300));
    stop(&mut child);
    let mut output = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut output)
        .unwrap();
    let report: serde_json::Value = serde_json::from_str(output.lines().last().unwrap()).unwrap();
    assert_eq!(report["totals"]["records"], 3);
    assert_eq!(report["totals"]["levels"]["error"], 1);
    assert_eq!(report["discarded_partial_records"], 1);
    assert_eq!(report["window"]["counts"]["records"], 0);
}

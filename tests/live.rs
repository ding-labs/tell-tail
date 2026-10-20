#![cfg(unix)]
use std::{
    fs::File,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_report(path: &Path, predicate: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        // A report in progress is not an acknowledgment; inspect complete lines.
        for line in text
            .split_inclusive('\n')
            .rev()
            .filter(|l| l.ends_with('\n'))
        {
            let value = serde_json::from_str(line).unwrap();
            if predicate(&value) {
                return value;
            }
        }
        assert!(
            Instant::now() < deadline,
            "expected report not observed: {text}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn stop(child: &mut Child, signal: &str, code: i32) {
    assert!(
        Command::new("kill")
            .args([signal, &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(code));
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(1),
            "shutdown timed out"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
fn append(path: &Path, bytes: &[u8]) {
    File::options()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
#[test]
fn idle_stdin_is_interruptible_and_reports_pending_bytes() {
    for (signal, code) in [("-TERM", 143), ("-INT", 130)] {
        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports.ndjson");
        let mut child = Process(
            Command::new(env!("CARGO_BIN_EXE_tell-tail"))
                .args(["--output", "json", "--interval-ms", "100"])
                .stdin(Stdio::piped())
                .stdout(File::create(&reports).unwrap())
                .spawn()
                .unwrap(),
        );
        child
            .0
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"WARN complete\nINFO partial")
            .unwrap();
        wait_report(&reports, |s| {
            s["totals"]["records"] == 1 && s["pending_bytes"] == 12
        });
        stop(&mut child.0, signal, code);
        let report = wait_report(&reports, |s| s["kind"] == "final");
        assert_eq!(report["state"], "stopped");
        assert_eq!(report["totals"]["records"], 1);
        assert_eq!(report["pending_bytes"], 12);
    }
}
#[test]
fn live_append_rotation_and_idle_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(&path, b"INFO old\n").unwrap();
    let reports = dir.path().join("reports.ndjson");
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_tell-tail"))
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
            .stdout(File::create(&reports).unwrap())
            .spawn()
            .unwrap(),
    );
    wait_report(&reports, |s| s["totals"]["records"] == 1);
    append(&path, b"WARN appended\nunfinished");
    wait_report(&reports, |s| {
        s["totals"]["records"] == 2 && s["pending_bytes"] == 10
    });
    std::fs::rename(&path, dir.path().join("old.log")).unwrap();
    std::fs::write(&path, b"ERROR rotated\n").unwrap();
    wait_report(&reports, |s| {
        s["totals"]["records"] == 3 && s["window"]["counts"]["records"] == 0
    });
    stop(&mut child.0, "-TERM", 143);
    let report = wait_report(&reports, |s| s["kind"] == "final");
    assert_eq!(report["totals"]["records"], 3);
    assert_eq!(report["totals"]["levels"]["error"], 1);
    assert_eq!(report["discarded_partial_records"], 1);
    assert_eq!(report["skipped_bytes"], 10);
}
#[test]
fn observed_truncation_and_reappearance_never_join_partial_generations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(&path, b"INFO old partial").unwrap();
    let reports = dir.path().join("reports.ndjson");
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .arg(&path)
            .args(["--output", "json", "--interval-ms", "100"])
            .stdout(File::create(&reports).unwrap())
            .spawn()
            .unwrap(),
    );
    wait_report(&reports, |s| s["state"] == "following");
    append(&path, b" suffix\nWARN new\nINFO pending");
    wait_report(&reports, |s| {
        s["totals"]["records"] == 1 && s["pending_bytes"] == 12
    });
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    wait_report(&reports, |s| {
        s["discarded_partial_records"] == 1 && s["pending_bytes"] == 0
    });
    append(&path, b"ERROR new generation\n");
    wait_report(&reports, |s| s["totals"]["records"] == 2);
    std::fs::remove_file(&path).unwrap();
    wait_report(&reports, |s| s["state"] == "waiting");
    std::fs::write(&path, b"FATAL reappeared\n").unwrap();
    wait_report(&reports, |s| s["totals"]["records"] == 3);
    stop(&mut child.0, "-TERM", 143);
    let report = wait_report(&reports, |s| s["kind"] == "final");
    assert_eq!(report["skipped_bytes"], 20);
    for level in ["warn", "error", "fatal"] {
        assert_eq!(report["totals"]["levels"][level], 1);
    }
    assert_eq!(report["totals"]["records"], 3);
}

#[cfg(target_os = "macos")]
fn apple_child(parent: u32) -> u32 {
    let output = Command::new("pgrep")
        .args(["-P", &parent.to_string()])
        .output()
        .unwrap();
    for pid in String::from_utf8(output.stdout).unwrap().split_whitespace() {
        let command = Command::new("ps")
            .args(["-o", "comm=", "-p", pid])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&command.stdout).trim() == "/usr/bin/log" {
            return pid.parse().unwrap();
        }
    }
    panic!("native log child not found");
}

#[cfg(target_os = "macos")]
#[test]
fn native_apple_capture_maps_real_events_ignores_stdin_eof_and_reaps_on_signals() {
    for (signal, code) in [("-TERM", 143), ("-INT", 130)] {
        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports.ndjson");
        let mut child = Process(
            Command::new(env!("CARGO_BIN_EXE_tell-tail"))
                .args([
                    "--apple-unified",
                    "--output",
                    "json",
                    "--interval-ms",
                    "100",
                ])
                .stdin(Stdio::null())
                .stdout(File::create(&reports).unwrap())
                .spawn()
                .unwrap(),
        );
        let frame = wait_report(&reports, |s| s["totals"]["records"].as_u64().unwrap() > 0);
        assert_eq!(frame["source"], "macOS unified log");
        assert_eq!(frame["state"], "reading");
        assert_eq!(frame["totals"]["parse_failures"], 0);
        let logger = apple_child(child.0.id());
        stop(&mut child.0, signal, code);
        assert_eq!(
            wait_report(&reports, |s| s["kind"] == "final")["state"],
            "stopped"
        );
        assert!(
            !Command::new("kill")
                .args(["-0", &logger.to_string()])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn native_apple_capture_failure_is_reported_instead_of_completion() {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let reports = dir.path().join("reports.ndjson");
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args([
                "--apple-unified",
                "--output",
                "json",
                "--interval-ms",
                "100",
            ])
            .stdin(Stdio::null())
            .stdout(File::create(&reports).unwrap())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    wait_report(&reports, |s| s["totals"]["records"].as_u64().unwrap() > 0);
    let logger = apple_child(child.0.id());
    assert!(
        Command::new("kill")
            .args(["-TERM", &logger.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let began = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(1));
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        wait_report(&reports, |s| s["kind"] == "final")["state"],
        "failed"
    );
    let mut diagnostic = String::new();
    child
        .0
        .stderr
        .as_mut()
        .unwrap()
        .read_to_string(&mut diagnostic)
        .unwrap();
    assert!(diagnostic.contains("Apple unified log stream ended unexpectedly"));
}

#![cfg(unix)]
use std::{
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
fn wait_for(path: &std::path::Path, predicate: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            // Every observed replacement must be a complete exposition.
            assert!(text.ends_with('\n'));
            assert_eq!(text.matches("tell_tail_records_total{level=").count(), 7);
            if predicate(&text) {
                return text;
            }
        }
        assert!(
            Instant::now() < deadline,
            "metrics did not reach expected state"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn missing_source_recovers_and_metrics_mark_orderly_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("future.log");
    let metrics = dir.path().join("metrics.prom");
    let mut child = Process(
        Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .arg(&input)
            .args([
                "--output",
                "prometheus",
                "--interval-ms",
                "100",
                "--metrics-file",
            ])
            .arg(&metrics)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for(&metrics, |s| {
        s.contains("tell_tail_source_up 0\n") && s.contains("tell_tail_running 1\n")
    });
    std::fs::write(&input, b"WARN recovered\n").unwrap();
    let first = wait_for(&metrics, |s| {
        s.contains("tell_tail_records_total{level=\"warn\"} 1\n")
            && s.contains("tell_tail_source_up 1\n")
    });
    wait_for(&metrics, |s| s != first);
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_for(&metrics, |s| s.contains("tell_tail_running 0\n"));
    assert_eq!(child.0.wait().unwrap().code(), Some(143));
    assert_eq!(std::fs::read_to_string(input).unwrap(), "WARN recovered\n");
}
#[test]
fn metrics_cannot_replace_input_through_a_path_alias() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("app.log");
    std::fs::write(&input, b"INFO preserved\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg(&input)
        .args(["--output", "prometheus", "--metrics-file"])
        .arg(dir.path().join("./app.log"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("input"));
    assert_eq!(std::fs::read_to_string(input).unwrap(), "INFO preserved\n");
}

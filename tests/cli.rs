use std::{
    io::Write,
    process::{Command, Stdio},
};
fn run(input: &[u8], extra: &[&str]) -> serde_json::Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .args(["--output", "json"])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn executable_counts_mixed_records_exactly() {
    let input =
        b"INFO mentions ERROR\n[WARN] slow\r\n{\"level\":\"ERROR\"}\n{bad}\n\nFATAL stopped";
    let s = run(input, &[]);
    assert_eq!(s["schema_version"], 1);
    assert_eq!(s["kind"], "final");
    assert_eq!(s["totals"]["records"], 6);
    assert_eq!(s["totals"]["levels"]["unknown"], 2);
    assert_eq!(s["totals"]["levels"]["info"], 1);
    assert_eq!(s["totals"]["parse_failures"], 1);
    assert_eq!(s["bytes_read"], input.len());
    assert_eq!(s["error_ratio"].as_f64().unwrap(), 2.0 / 6.0);
}
#[test]
fn empty_input_and_oversize_have_defined_denominators() {
    let empty = run(b"", &[]);
    assert!(empty["error_ratio"].is_null());
    assert_eq!(empty["totals"]["records"], 0);
    let large = run(b"WARN too long\nINFO\n", &["--max-record-bytes", "4"]);
    assert_eq!(large["totals"]["records"], 2);
    assert_eq!(large["totals"]["oversized_records"], 1);
    assert_eq!(large["totals"]["levels"]["info"], 1);
}

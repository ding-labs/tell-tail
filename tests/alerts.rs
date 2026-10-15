use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

const INPUT: &[u8] = b"INFO private details\nFATAL raw log secret\nWARN slow\nunknown\n";

struct Monitor(Option<Child>);
impl Monitor {
    fn start(listener: &TcpListener, extra: &[&str]) -> Self {
        Self::start_mode(listener, "json", extra)
    }
    fn start_mode(listener: &TcpListener, mode: &str, extra: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args([
                "--output",
                mode,
                "--interval-ms",
                "60000",
                "--alert-webhook",
                &format!("http://{}/secret-endpoint", listener.local_addr().unwrap()),
                "--alert-error-percent",
                "25",
                "--alert-min-records",
                "4",
                "--alert-cooldown",
                "0",
                "--alert-message",
                "Check \"service\"\nnow",
            ])
            .args(extra)
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("ALL_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy")
            .env_remove("all_proxy")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        Self(Some(child))
    }
    fn write(&mut self, input: &[u8]) {
        self.0
            .as_mut()
            .unwrap()
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input)
            .unwrap();
    }
    fn close_input(&mut self) {
        drop(self.0.as_mut().unwrap().stdin.take());
    }
    fn finish(mut self) -> Output {
        self.close_input();
        let started = Instant::now();
        while self.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "monitor did not exit"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        self.0.take().unwrap().wait_with_output().unwrap()
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn listener() -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}

fn receive(listener: &TcpListener) -> (TcpStream, serde_json::Value) {
    let started = Instant::now();
    let stream = loop {
        if let Ok((stream, _)) = listener.accept() {
            break stream;
        }
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "no webhook received"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let length = length.unwrap();
    assert!(length < 4096);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (reader.into_inner(), serde_json::from_slice(&body).unwrap())
}

#[test]
fn exact_threshold_posts_a_custom_message_at_finite_eof() {
    let server = listener();
    let mut monitor = Monitor::start(&server, &[]);
    monitor.write(INPUT);
    monitor.close_input();
    let (mut stream, payload) = receive(&server);
    stream
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    let output = monitor.finish();
    assert!(output.status.success());
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["event"], "error_threshold_met");
    assert_eq!(payload["error_percent"], 25.0);
    assert_eq!(payload["threshold_percent"], 25.0);
    assert_eq!(payload["records"], 4);
    assert_eq!(payload["error_records"], 1);
    assert_eq!(payload["window_seconds"], 60);
    assert_eq!(payload["source"], "stdin");
    assert_eq!(payload["message"], "Check \"service\"\nnow");
    assert!(!payload.to_string().contains("raw log secret"));
    assert!(!payload.to_string().contains("secret-endpoint"));
    let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["session_id"], snapshot["session_id"]);
}

#[test]
fn failed_delivery_does_not_repeat_or_fail_the_monitor() {
    let server = listener();
    let mut monitor = Monitor::start(&server, &[]);
    monitor.write(INPUT);
    let (mut stream, _) = receive(&server);
    stream
        .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(350));
    assert!(server.accept().is_err(), "sustained breach must not repeat");
    let output = monitor.finish();
    assert!(output.status.success());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("non-2xx HTTP response"), "{diagnostic}");
    assert!(!diagnostic.contains("secret-endpoint"));
}

#[test]
fn stalled_endpoint_does_not_block_ingestion_or_shutdown() {
    let server = listener();
    let mut monitor = Monitor::start(&server, &[]);
    monitor.write(INPUT);
    let (_stream, _) = receive(&server);
    monitor.write(&b"INFO more\n".repeat(200_000));
    let started = Instant::now();
    let output = monitor.finish();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(output.status.success());
    let snapshot: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(snapshot["totals"]["records"], 200_004);
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("shutdown deadline reached")
    );
}

#[test]
fn too_few_records_do_not_send_a_webhook() {
    let server = listener();
    let mut monitor = Monitor::start(&server, &[]);
    monitor.write(b"ERROR one\nERROR two\nERROR three\n");
    assert!(monitor.finish().status.success());
    assert!(server.accept().is_err());
}

#[test]
fn alerts_work_with_text_prometheus_and_metrics_files() {
    for (mode, metrics_file) in [("text", false), ("prometheus", false), ("prometheus", true)] {
        let server = listener();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metrics.prom");
        let extra = if metrics_file {
            vec!["--metrics-file", path.to_str().unwrap()]
        } else {
            vec![]
        };
        let mut monitor = Monitor::start_mode(&server, mode, &extra);
        monitor.write(INPUT);
        let (mut stream, payload) = receive(&server);
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        let output = monitor.finish();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(payload["error_percent"], 25.0);
        if metrics_file {
            assert!(output.stdout.is_empty());
            assert!(
                std::fs::read_to_string(path)
                    .unwrap()
                    .contains("tell_tail_running 0\n")
            );
        } else {
            assert!(!output.stdout.is_empty());
        }
    }
}

#[test]
fn stalled_delivery_preserves_signal_exit_codes_and_deadlines() {
    for (signal, code) in [("-TERM", 143), ("-INT", 130)] {
        let server = listener();
        let mut monitor = Monitor::start(&server, &[]);
        monitor.write(INPUT);
        let (_stream, _) = receive(&server);
        let started = Instant::now();
        assert!(
            Command::new("kill")
                .args([signal, &monitor.0.as_ref().unwrap().id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let output = monitor.finish();
        assert_eq!(output.status.code(), Some(code));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

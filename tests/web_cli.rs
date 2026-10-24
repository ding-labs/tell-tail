#![cfg(feature = "web")]
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Monitor {
    child: Child,
    address: SocketAddr,
}

#[cfg(target_os = "macos")]
#[test]
fn native_apple_logs_stream_into_the_dashboard_without_stdin() {
    let mut monitor = Monitor::start(&["--apple-unified"]);
    drop(monitor.child.stdin.take());
    let frame = monitor.until(|f| f["metrics"]["totals"]["records"].as_str().unwrap() != "0");
    assert_eq!(frame["metrics"]["source"], "macOS unified log");
    assert_eq!(frame["metrics"]["state"], "reading");
    assert_eq!(frame["metrics"]["totals"]["parse_failures"], "0");
    monitor.signal("-TERM", 143);
}

impl Monitor {
    fn start(extra: &[&str]) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["--output", "web", "--interval-ms", "60000"])
            .args(extra)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut monitor = Self {
            child,
            address: "127.0.0.1:0".parse().unwrap(),
        };
        let stderr = monitor.child.stderr.as_mut().unwrap();
        let flags = rustix::fs::fcntl_getfl(&*stderr).unwrap();
        rustix::fs::fcntl_setfl(&*stderr, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
        let began = Instant::now();
        let mut text = String::new();
        let address = loop {
            let mut bytes = [0; 4096];
            match stderr.read(&mut bytes) {
                Ok(n) => text.push_str(std::str::from_utf8(&bytes[..n]).unwrap()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) => panic!("{e}"),
            }
            if let Some(url) = text
                .split_whitespace()
                .find(|s| s.starts_with("http://") && s.ends_with('/'))
            {
                break url
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .parse()
                    .unwrap();
            }
            assert!(began.elapsed() < Duration::from_secs(3), "no URL: {text}");
            std::thread::sleep(Duration::from_millis(5));
        };
        monitor.address = address;
        monitor
    }

    fn connect(&self, path: &str, extra: &str) -> BufReader<TcpStream> {
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(1)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: {}\r\n{extra}\r\n",
                    self.address
                )
                .as_bytes(),
            )
            .unwrap();
        BufReader::new(stream)
    }

    fn snapshot(&self) -> serde_json::Value {
        let mut response = String::new();
        self.connect("/api/snapshot", "")
            .read_to_string(&mut response)
            .unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    fn until(&self, test: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let started = Instant::now();
        loop {
            let frame = self.snapshot();
            if test(&frame) {
                return frame;
            }
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "last frame: {frame}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn signal(&mut self, signal: &str, code: i32) {
        let started = Instant::now();
        assert!(
            Command::new("kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(code));
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "shutdown deadline exceeded"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut stdout = Vec::new();
        self.child
            .stdout
            .as_mut()
            .unwrap()
            .read_to_end(&mut stdout)
            .unwrap();
        assert!(stdout.is_empty(), "web mode wrote periodic stdout");
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn event(reader: &mut impl BufRead) -> (String, serde_json::Value) {
    let mut id = String::new();
    loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "stream closed before frame"
        );
        if let Some(value) = line.strip_prefix("id: ") {
            id = value.trim().into();
        }
        if let Some(value) = line.strip_prefix("data: ") {
            return (id, serde_json::from_str(value).unwrap());
        }
    }
}

#[test]
fn stdin_eof_retains_exact_final_frames_and_reconnects_to_current_state() {
    let mut monitor = Monitor::start(&[]);
    let initial = monitor.snapshot();
    assert_eq!(initial["web_schema_version"], 1);
    assert_eq!(initial["metrics"]["totals"]["records"], "0");
    let mut live = monitor.connect("/api/events", "");
    let (_, first) = event(&mut live);
    monitor
        .child
        .stdin
        .take()
        .unwrap()
        .write_all(b"INFO one\nWARN unterminated")
        .unwrap();
    let final_frame = loop {
        let (_, frame) = event(&mut live);
        if frame["metrics"]["state"] == "complete" {
            break frame;
        }
    };
    assert_eq!(final_frame["metrics"]["kind"], "final");
    assert_eq!(final_frame["metrics"]["totals"]["records"], "2");
    assert_eq!(final_frame["metrics"]["totals"]["levels"]["warn"], "1");
    assert_ne!(first["sequence"], final_frame["sequence"]);
    std::thread::sleep(Duration::from_millis(150));
    assert!(monitor.child.try_wait().unwrap().is_none());
    assert_eq!(monitor.snapshot(), final_frame);
    let (id, reconnected) =
        event(&mut monitor.connect("/api/events", "Last-Event-ID: unrelated:999999\r\n"));
    assert_eq!(reconnected, final_frame);
    assert_eq!(
        id,
        format!(
            "{}:{}",
            final_frame["metrics"]["session_id"].as_str().unwrap(),
            final_frame["sequence"].as_str().unwrap()
        )
    );
    monitor.signal("-INT", 130);
}

#[test]
fn signals_stop_live_ingestion_and_deliver_final_state_within_one_second() {
    for (signal, code) in [("-INT", 130), ("-TERM", 143)] {
        let mut monitor = Monitor::start(&[]);
        monitor
            .child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"INFO live\n")
            .unwrap();
        monitor.until(|frame| frame["metrics"]["totals"]["records"] == "1");
        let mut live = monitor.connect("/api/events", "");
        event(&mut live);
        // Also leave a partial request header pending during shutdown.
        let mut slow = TcpStream::connect(monitor.address).unwrap();
        slow.write_all(b"GET / HTTP/1.1\r\nHost: ").unwrap();
        monitor.signal(signal, code);
        let (_, final_frame) = event(&mut live);
        assert_eq!(final_frame["metrics"]["state"], "stopped");
        assert_eq!(final_frame["metrics"]["totals"]["records"], "1");
    }
}

#[test]
fn bind_failure_precedes_input_start_and_invalid_web_arguments_exit_two() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
        .arg(directory.path())
        .args([
            "--output",
            "web",
            "--web-port",
            &listener.local_addr().unwrap().port().to_string(),
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    // glibc and musl use different wording for the same occupied-port error.
    let expected = TcpListener::bind(listener.local_addr().unwrap()).unwrap_err();
    assert!(String::from_utf8_lossy(&output.stderr).contains(&expected.to_string()));
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!(
        "cannot bind web listener at {}",
        listener.local_addr().unwrap()
    )));
    for args in [
        vec!["--web-port", "0"],
        vec!["--output", "web", "--web-port", "65536"],
        vec!["input.log", "--output", "web", "--once"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[test]
fn followed_file_updates_and_input_failure_publish_a_failed_final_frame() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    std::fs::write(&path, b"INFO first\n").unwrap();
    let mut monitor = Monitor::start(&[path.to_str().unwrap(), "--from", "start"]);
    monitor.until(|frame| frame["metrics"]["totals"]["records"] == "1");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"WARN appended\n")
        .unwrap();
    monitor.until(|frame| frame["metrics"]["totals"]["records"] == "2");
    let mut events = monitor.connect("/api/events", "");
    event(&mut events);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    loop {
        let (_, frame) = event(&mut events);
        if frame["metrics"]["state"] == "failed" {
            assert_eq!(frame["metrics"]["kind"], "final");
            assert_eq!(frame["metrics"]["totals"]["records"], "2");
            break;
        }
    }
    let began = Instant::now();
    loop {
        if let Some(status) = monitor.child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(1));
            break;
        }
        assert!(began.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn webhook_alerts_continue_to_observe_native_publications_in_web_mode() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/alert", listener.local_addr().unwrap());
    let mut monitor = Monitor::start(&[
        "--alert-webhook",
        &endpoint,
        "--alert-error-percent",
        "25",
        "--alert-min-records",
        "4",
    ]);
    monitor
        .child
        .stdin
        .take()
        .unwrap()
        .write_all(b"INFO one\nFATAL two\nWARN three\nunknown\n")
        .unwrap();
    let began = Instant::now();
    let stream = loop {
        if let Ok((stream, _)) = listener.accept() {
            break stream;
        }
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "no webhook received"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut length = None;
    loop {
        let mut line = String::new();
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
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    let alert: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(alert["records"], 4);
    assert_eq!(alert["error_records"], 1);
    reader
        .get_mut()
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    monitor.until(|frame| frame["metrics"]["state"] == "complete");
    monitor.signal("-TERM", 143);
}

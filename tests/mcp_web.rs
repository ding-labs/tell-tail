#![cfg(feature = "web")]
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Monitor {
    child: Child,
    address: SocketAddr,
}
impl Monitor {
    fn start(dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["mcp", "--source", "codex-desktop", "--log-dir"])
            .arg(dir)
            .args(["--output", "web"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.as_mut().unwrap();
        let flags = rustix::fs::fcntl_getfl(&*stderr).unwrap();
        rustix::fs::fcntl_setfl(&*stderr, flags | rustix::fs::OFlags::NONBLOCK).unwrap();
        let mut diagnostic = String::new();
        let began = Instant::now();
        let address = loop {
            let mut bytes = [0; 4096];
            match stderr.read(&mut bytes) {
                Ok(n) => diagnostic.push_str(std::str::from_utf8(&bytes[..n]).unwrap()),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) => panic!("{e}"),
            };
            if let Some(url) = diagnostic
                .split_whitespace()
                .find(|s| s.starts_with("http://"))
            {
                break url
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .parse()
                    .unwrap();
            }
            if began.elapsed() > Duration::from_secs(3) {
                let _ = child.kill();
                panic!("no URL: {diagnostic}");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        Self { child, address }
    }
    fn request(&self, path: &str, headers: &str) -> String {
        let mut stream = TcpStream::connect(self.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {}\r\n{headers}\r\n",
            self.address
        )
        .unwrap();
        let mut body = String::new();
        stream.read_to_string(&mut body).unwrap();
        body
    }
    fn json(&self, path: &str) -> serde_json::Value {
        let r = self.request(path, "");
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        serde_json::from_str(r.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
    fn until(&self, predicate: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let t = Instant::now();
        loop {
            let f = self.json("/api/mcp/snapshot");
            if predicate(&f) {
                return f;
            }
            assert!(t.elapsed() < Duration::from_secs(4), "{f}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn live_appends_have_safe_details_and_scoped_recovery() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("desktop.log");
    fs::write(&path, "").unwrap();
    let m = Monitor::start(t.path());
    m.until(|s| s["sources"].as_array().is_some_and(|s| s.len() == 1));
    let fail = "2026-11-12T00:00:00Z warning [AppServerConnection] mcp_server_startup_status_updated server=example status=failed failureReason=reauthenticationRequired error=\"PRIVATE_SECRET\" threadId=PRIVATE_SCOPE\n";
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(fail.as_bytes())
        .unwrap();
    let f = m.until(|s| s["counts"]["observations"] == 1);
    assert_eq!(f["issues"][0]["reason"], "reauthentication");
    assert_eq!(f["events"][0]["historical"], false);
    assert!(!f.to_string().contains("PRIVATE"));
    let e = m.json("/api/mcp/event/1");
    assert_eq!(e["payload_available"], false);
    assert_eq!(e["evidence"][0]["offset"], 0);
    let filtered = m.json("/api/mcp/history?q=reauth&server=example&category=failure");
    assert_eq!(filtered["matched"], 1);
    fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"2026-11-12T00:00:01Z info [AppServerConnection] mcp_server_startup_status_updated server=example status=ready error=null threadId=PRIVATE_SCOPE\n").unwrap();
    let f = m.until(|s| s["counts"]["observations"] == 2);
    assert!(f["issues"][0]["recovered_ms"].is_number());
}
#[test]
fn access_checks_and_unknown_details_are_explicit() {
    let t = tempfile::tempdir().unwrap();
    let m = Monitor::start(t.path());
    assert!(
        m.request("/api/mcp/snapshot", "Origin: https://untrusted.example\r\n")
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        m.request("/api/mcp/snapshot", "Host: untrusted.example\r\n")
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        m.request("/api/mcp/event/999", "")
            .starts_with("HTTP/1.1 410")
    );
    assert!(
        m.request("/api/mcp/history?q=%ff", "")
            .starts_with("HTTP/1.1 400")
    );
    let r = m.request("/api/mcp/snapshot", "");
    assert!(r.contains("content-security-policy:"));
    assert!(r.contains("cache-control: no-store"));
}
#[test]
fn new_files_and_bounded_history_pagination_work() {
    let t = tempfile::tempdir().unwrap();
    let m = Monitor::start(t.path());
    m.until(|s| !s["roots"].as_array().unwrap().is_empty());
    let line = "2026-11-12T00:00:00Z info [AppServerConnection] mcp_server_startup_status_updated server=example status=ready threadId=synthetic\n";
    fs::write(t.path().join("new.log"), line.repeat(110)).unwrap();
    m.until(|s| s["counts"]["observations"] == 110);
    let first = m.json("/api/mcp/history");
    assert_eq!(first["events"].as_array().unwrap().len(), 100);
    let next = first["next_before"].as_u64().unwrap();
    let second = m.json(&format!("/api/mcp/history?before={next}"));
    assert_eq!(second["events"].as_array().unwrap().len(), 10);
}

#[test]
fn idle_publication_coalesces_and_stream_capacity_is_bounded() {
    let t = tempfile::tempdir().unwrap();
    let m = Monitor::start(t.path());
    let first = m.until(|s| !s["roots"].as_array().unwrap().is_empty());
    std::thread::sleep(Duration::from_millis(450));
    let idle = m.json("/api/mcp/snapshot");
    assert_eq!(idle["sequence"], first["sequence"]);
    assert_eq!(idle["observed_ms"], first["observed_ms"]);
    let mut streams = vec![];
    for _ in 0..4 {
        let mut stream = TcpStream::connect(m.address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            stream,
            "GET /api/mcp/events HTTP/1.1\r\nHost: {}\r\n\r\n",
            m.address
        )
        .unwrap();
        let mut bytes = [0; 8192];
        let n = stream.read(&mut bytes).unwrap();
        assert!(
            std::str::from_utf8(&bytes[..n])
                .unwrap()
                .starts_with("HTTP/1.1 200")
        );
        streams.push(stream);
    }
    assert!(m.request("/api/mcp/events", "").starts_with("HTTP/1.1 429"));
    fs::write(t.path().join("new.log"), "2026-11-12T00:00:00Z info [AppServerConnection] mcp_server_startup_status_updated server=example status=failed\n").unwrap();
    assert_eq!(
        m.until(|s| s["counts"]["observations"] == 1)["retained_issues"],
        1
    );
    drop(streams);
}

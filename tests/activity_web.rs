#![cfg(feature = "web")]
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Monitor {
    child: Child,
    address: SocketAddr,
}
impl Monitor {
    fn start(dir: &std::path::Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let child = Command::new(env!("CARGO_BIN_EXE_tell-tail"))
            .args(["desktop", "--source", "codex-desktop", "--log-dir"])
            .arg(dir)
            .args(["--output", "web", "--web-port", &address.port().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let m = Self { child, address };
        let began = Instant::now();
        while m.request("/api/activity/snapshot", "").is_err() {
            assert!(began.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(20));
        }
        m
    }
    fn request(&self, path: &str, extra: &str) -> std::io::Result<String> {
        let mut socket = TcpStream::connect(self.address)?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        write!(
            socket,
            "GET {path} HTTP/1.1\r\nHost: {}\r\n{extra}\r\n",
            self.address
        )?;
        let mut response = String::new();
        socket.read_to_string(&mut response)?;
        Ok(response)
    }
    fn json(&self, path: &str) -> serde_json::Value {
        let r = self.request(path, "").unwrap();
        assert!(r.starts_with("HTTP/1.1 200"), "{r}");
        serde_json::from_str(r.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
    fn until(&self, n: u64) -> serde_json::Value {
        let began = Instant::now();
        loop {
            let s = self.json("/api/activity/snapshot");
            if s["retained_items"].as_u64().unwrap() >= n {
                return s;
            }
            assert!(began.elapsed() < Duration::from_secs(5), "{s}");
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
fn activity_api_preserves_freeze_filter_bins_provenance_and_access_rules() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("desktop.log");
    fs::write(&path, include_str!("fixtures/desktop-activity.log")).unwrap();
    let m = Monitor::start(dir.path());
    let snapshot = m.until(14);
    let at = snapshot["watermark"].as_str().unwrap();
    let epoch = snapshot["collector_epoch"].as_str().unwrap();
    let query = format!("epoch={epoch}&as_of={at}");
    let page = m.json(&format!("/api/activity/items?{query}"));
    assert_eq!(page["matched"], 12);
    assert_eq!(page["diagnostics_hidden"], 2);
    assert!(!page.to_string().contains("PRIVATE"));
    let bins = m.json(&format!("/api/activity/bins?{query}"));
    let count: u64 = bins["lanes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| v["counts"].as_array().unwrap())
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert_eq!(count, 12);
    let groups = m.json(&format!("/api/activity/problems?{query}"));
    assert_eq!(groups["matched"], 1);
    assert!(groups["groups"][0]["recovered_ms"].is_number());
    let id = groups["groups"][0]["item_id"].as_str().unwrap();
    let detail = m.json(&format!("/api/activity/item/{id}?{query}"));
    assert!(detail["evidence"][0]["offset"].is_string());
    assert!(!detail.to_string().contains("PRIVATE"));
    let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
    for _ in 0..125 {
        writeln!(
            file,
            "2026-11-19T12:01:00Z info [electron-message-handler] chatgpt_pubsub_transport_opened"
        )
        .unwrap();
    }
    m.until(139);
    assert_eq!(
        m.json(&format!("/api/activity/items?{query}"))["matched"],
        12
    );
    let live = m.json("/api/activity/items?diagnostics=true");
    assert_eq!(live["matched"], 139);
    let cursor = live["next_cursor"].as_str().unwrap();
    let watermark = live["as_of"].as_str().unwrap();
    let next = m.json(&format!(
        "/api/activity/items?diagnostics=true&as_of={watermark}&cursor={cursor}"
    ));
    assert_eq!(
        live["items"].as_array().unwrap().len() + next["items"].as_array().unwrap().len(),
        139
    );
    for path in [
        "/api/activity/items?category=bad",
        "/api/activity/items?q=x&q=y",
        "/api/activity/items?since=-1",
        "/api/activity/snapshot?private=secret",
    ] {
        assert!(m.request(path, "").unwrap().starts_with("HTTP/1.1 400"));
    }
    assert!(
        m.request("/api/activity/items?epoch=old", "")
            .unwrap()
            .starts_with("HTTP/1.1 410")
    );
    assert!(
        m.request(
            "/api/activity/snapshot",
            "Origin: https://untrusted.example\r\n"
        )
        .unwrap()
        .starts_with("HTTP/1.1 403")
    );
    assert!(
        m.request("/api/activity/item/999999", "")
            .unwrap()
            .starts_with("HTTP/1.1 410")
    );
}

#[test]
fn desktop_logs_reuse_real_metrics_and_keep_raw_bodies_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("desktop.log");
    fs::write(&path, "INFO ready\nERROR PRIVATE_SECRET\n").unwrap();
    let m = Monitor::start(dir.path());
    let began = Instant::now();
    let snapshot = loop {
        let s = m.json("/api/snapshot");
        if s["metrics"]["totals"]["records"] == "2" {
            break s;
        }
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(!snapshot.to_string().contains("PRIVATE"));
    assert_eq!(snapshot["metrics"]["totals"]["levels"]["error"], "1");
    let page = m.request("/logs", "").unwrap();
    assert!(page.contains("data-collector=\"desktop\""));
    assert!(page.contains("Throughput"));
    assert!(
        m.request("/api/snapshot", "Origin: https://foreign.example\r\n")
            .unwrap()
            .starts_with("HTTP/1.1 403")
    );
    fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(b"WARN next\n")
        .unwrap();
    let began = Instant::now();
    while m.json("/api/snapshot")["metrics"]["totals"]["records"] != "3" {
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
}

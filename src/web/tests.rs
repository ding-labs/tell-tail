use super::*;
use crate::{
    snapshots::{SnapshotReader, SnapshotSlot},
    stats::Stats,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

#[tokio::test(flavor = "current_thread")]
async fn dashboard_assets_are_embedded_fixed_routes_with_local_security_headers() {
    let h = Harness::new("stdin".into());
    for (path, mime, marker) in [
        ("/", "text/html", "id=\"record-rate\""),
        ("/dashboard.css", "text/css", ".metrics-grid"),
        ("/dashboard.mjs", "text/javascript", "./frame.mjs"),
        ("/frame.mjs", "text/javascript", "parseFrame"),
        ("/render.mjs", "text/javascript", "textContent"),
        ("/logo.svg", "image/svg+xml", "<svg"),
        ("/favicon.svg", "image/svg+xml", "<svg"),
    ] {
        let response = h.get(path).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{path}: {response}");
        assert!(response.contains(&format!("content-type: {mime}")));
        assert!(response.contains("cache-control: no-store"));
        assert!(response.contains("script-src 'self'"));
        assert!(response.contains(marker));
    }
    for path in ["/web/index.html", "/../Cargo.toml", "/missing.mjs"] {
        assert!(h.get(path).await.starts_with("HTTP/1.1 404"));
    }
    h.finish().await.unwrap();
}

struct Harness {
    address: SocketAddr,
    slot: Arc<SnapshotSlot>,
    stop: Arc<AtomicUsize>,
    task: Option<tokio::task::JoinHandle<io::Result<()>>>,
    probe: Arc<socket::Probe>,
}

impl Harness {
    fn new(source: String) -> Self {
        let server = Server::bind(0).unwrap();
        let address = server.address().unwrap();
        let probe = server.probe.clone();
        let slot = Arc::new(SnapshotSlot::new(Stats::new(60, source).snapshot(
            Duration::ZERO,
            InputState::Reading,
            0,
            None,
        )));
        let stop = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn(server.serve(SnapshotReader { slot: slot.clone() }, stop.clone()));
        Self {
            address,
            slot,
            stop,
            task: Some(task),
            probe,
        }
    }

    fn current(&self) -> crate::stats::Snapshot {
        SnapshotReader {
            slot: self.slot.clone(),
        }
        .latest()
        .snapshot
    }

    async fn request(&self, request: &str) -> String {
        let mut socket = TcpStream::connect(self.address).await.unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        let result = timeout(Duration::from_secs(3), socket.read_to_string(&mut response))
            .await
            .unwrap();
        if let Err(error) = result {
            assert_eq!(error.kind(), io::ErrorKind::ConnectionReset, "{response}");
        }
        response
    }

    async fn get(&self, path: &str) -> String {
        self.request(&format!(
            "GET {path} HTTP/1.1\r\nHost: {}\r\n\r\n",
            self.address
        ))
        .await
    }

    async fn events(&self, small_buffer: bool) -> (String, BufReader<TcpStream>) {
        let mut socket = TcpStream::connect(self.address).await.unwrap();
        if small_buffer {
            rustix::net::sockopt::set_socket_recv_buffer_size(&socket, 1024).unwrap();
        }
        socket
            .write_all(
                format!("GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", self.address).as_bytes(),
            )
            .await
            .unwrap();
        let mut reader = BufReader::new(socket);
        let mut header = String::new();
        loop {
            let mut line = String::new();
            assert!(
                timeout(Duration::from_secs(1), reader.read_line(&mut line))
                    .await
                    .unwrap()
                    .unwrap()
                    > 0
            );
            header.push_str(&line);
            if line == "\r\n" {
                return (header, reader);
            }
        }
    }

    async fn finish(mut self) -> io::Result<()> {
        self.stop.store(1, Ordering::Relaxed);
        timeout(Duration::from_secs(1), self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

async fn next_data(reader: &mut BufReader<TcpStream>) -> serde_json::Value {
    timeout(Duration::from_secs(1), async {
        loop {
            let mut line = String::new();
            assert!(
                reader.read_line(&mut line).await.unwrap() > 0,
                "closed before event"
            );
            if let Some(data) = line.strip_prefix("data: ") {
                return serde_json::from_str(data).unwrap();
            }
        }
    })
    .await
    .unwrap()
}

#[test]
fn default_http_port_accepts_browser_authorities_without_weakening_origin_checks() {
    for host in ["127.0.0.1", "127.0.0.1:80"] {
        for origin in ["http://127.0.0.1", "http://127.0.0.1:80"] {
            let request = Request::builder()
                .uri("/api/snapshot")
                .header(header::HOST, host)
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            assert_eq!(request_status(&request, "127.0.0.1:80"), None);
            assert_eq!(
                request_status(&request, "127.0.0.1:8765"),
                Some(StatusCode::FORBIDDEN)
            );
        }
    }
    for origin in [
        "https://127.0.0.1",
        "http://127.0.0.1:81",
        "http://localhost",
        "null",
    ] {
        let request = Request::builder()
            .uri("/api/snapshot")
            .header(header::HOST, "127.0.0.1")
            .header(header::ORIGIN, origin)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            request_status(&request, "127.0.0.1:80"),
            Some(StatusCode::FORBIDDEN)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn stream_capacity_is_independent_of_snapshot_requests_and_released_on_disconnect() {
    let harness = Harness::new("stdin".into());
    assert!(
        harness
            .get("/api/snapshot")
            .await
            .contains("x-tell-tail-streams-available: 4")
    );
    let mut readers = Vec::new();
    for _ in 0..STREAMS {
        let (header, reader) = harness.events(false).await;
        assert!(header.starts_with("HTTP/1.1 200 OK"));
        readers.push(reader);
    }
    assert!(
        harness
            .events(false)
            .await
            .0
            .starts_with("HTTP/1.1 429 Too Many Requests")
    );
    let snapshot = harness.get("/api/snapshot").await;
    assert!(snapshot.starts_with("HTTP/1.1 200 OK"));
    assert!(snapshot.contains("x-tell-tail-streams-available: 0"));
    drop(readers.pop());
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let (header, reader) = harness.events(false).await;
        if header.starts_with("HTTP/1.1 200 OK") {
            readers.push(reader);
            break;
        }
        assert!(Instant::now() < deadline, "stream slot leaked");
        sleep(Duration::from_millis(10)).await;
    }
    harness.finish().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn connection_admission_and_header_deadlines_bound_incomplete_requests() {
    let harness = Harness::new("stdin".into());
    let mut sockets = Vec::new();
    for _ in 0..CONNECTIONS {
        let mut socket = TcpStream::connect(harness.address).await.unwrap();
        socket.write_all(b"GET / HTTP/1.1\r\nHost: ").await.unwrap();
        sockets.push(socket);
    }
    sleep(Duration::from_millis(100)).await;
    let mut extra = TcpStream::connect(harness.address).await.unwrap();
    let closed = timeout(Duration::from_millis(500), extra.read(&mut [0; 1]))
        .await
        .unwrap();
    assert!(
        matches!(closed, Ok(0) | Err(_)),
        "17th connection was admitted"
    );
    for socket in &mut sockets {
        let mut bytes = Vec::new();
        let result = timeout(
            IO_TIMEOUT + Duration::from_secs(1),
            socket.read_to_end(&mut bytes),
        )
        .await
        .unwrap();
        if let Err(error) = result {
            assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        }
    }
    assert!(
        harness
            .get("/api/snapshot")
            .await
            .starts_with("HTTP/1.1 200 OK")
    );
    harness.finish().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_large_and_foreign_requests_do_not_reach_data_routes() {
    let harness = Harness::new("stdin".into());
    for (request, expected) in [
        (
            format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: {}\r\nX-Fill: {}\r\n\r\n",
                harness.address,
                "x".repeat(HEADER_BYTES)
            ),
            "431",
        ),
        ("not HTTP\r\n\r\n".into(), "400"),
        (
            format!(
                "GET http://foreign.test/api/snapshot HTTP/1.1\r\nHost: {}\r\n\r\n",
                harness.address
            ),
            "400",
        ),
        (
            format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: {}\r\nTransfer-Encoding: chunked\r\n\r\n",
                harness.address
            ),
            "400",
        ),
        (
            format!(
                "GET /api/events HTTP/1.1\r\nHost: {}\r\nOrigin: null\r\n\r\n",
                harness.address
            ),
            "403",
        ),
        (
            format!(
                "GET /api/snapshot HTTP/1.1\r\nHost: {}\r\nOrigin: http://{}\r\nAccept-Encoding: gzip\r\n\r\n",
                harness.address, harness.address
            ),
            "200",
        ),
    ] {
        let response = harness.request(&request).await;
        assert!(
            (response.is_empty() && (expected == "431" || request.starts_with("not HTTP")))
                || response.starts_with(&format!("HTTP/1.1 {expected}")),
            "{response}"
        );
        assert!(!response.contains("access-control-allow-origin:"));
        assert!(!response.contains("content-encoding:"));
    }
    harness.finish().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn fatal_publications_and_worker_closure_attempt_final_failure_delivery() {
    for mode in ["failed", "oversize", "panic"] {
        let mut harness = Harness::new("stdin".into());
        let (_, mut reader) = harness.events(false).await;
        next_data(&mut reader).await;
        match mode {
            "panic" => drop(crate::snapshots::CloseSnapshots(harness.slot.clone())),
            _ => {
                let mut snapshot = harness.current();
                if mode == "failed" {
                    snapshot.state = InputState::Failed;
                    snapshot.kind = "final";
                } else {
                    snapshot.source = "x".repeat(crate::web_frame::MAX_FRAME_BYTES);
                }
                harness.slot.publish(snapshot);
            }
        }
        let final_data = next_data(&mut reader).await;
        if mode == "failed" {
            assert_eq!(final_data["metrics"]["state"], "failed");
            assert_eq!(final_data["metrics"]["kind"], "final");
        } else {
            assert_eq!(
                final_data,
                serde_json::json!({"message": "web output failed"})
            );
        }
        let result = timeout(Duration::from_secs(1), harness.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
        let error = result.unwrap_err();
        if mode == "oversize" {
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("web frame sequence 1 at"));
            assert!(error.to_string().contains("exceeds 65536 JSON bytes"));
            assert!(!error.to_string().contains(&"x".repeat(100)));
        }
        assert_eq!(harness.stop.load(Ordering::Relaxed), 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn an_undrained_partially_sent_event_expires_without_blocking_other_clients_or_shutdown() {
    // Valid maximum-width source text forces real kernel backpressure without a
    // production padding flag, browser reader emulation, or an unbounded queue.
    let harness = Harness::new("x".repeat(60_000));
    let (header, _unread) = harness.events(true).await;
    assert!(header.starts_with("HTTP/1.1 200 OK"));
    let slot = harness.slot.clone();
    let mut snapshot = harness.current();
    let publisher = tokio::spawn(async move {
        for n in 1..=200 {
            snapshot.totals.records = n;
            snapshot.totals.levels.0[crate::parse::Level::Info as usize] = n;
            slot.publish(snapshot.clone());
            sleep(Duration::from_millis(20)).await;
        }
    });
    sleep(Duration::from_millis(500)).await;
    // Observe the real TcpStream's poll_write, below Hyper's response stream.
    assert!(harness.probe.written.load(Ordering::Relaxed) > header.len());
    assert!(
        harness.probe.partial.load(Ordering::Relaxed) > 0,
        "fixture did not force a partial socket write"
    );
    assert!(
        harness.probe.blocked.load(Ordering::Relaxed),
        "the real socket did not block"
    );
    let response = harness.get("/api/snapshot").await;
    let frame: serde_json::Value =
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(
        frame["metrics"]["totals"]["records"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 10
    );
    sleep(Duration::from_millis(2200)).await;
    assert_eq!(harness.probe.expired.load(Ordering::Relaxed), 1);
    let mut readers = Vec::new();
    for _ in 0..STREAMS {
        let (header, reader) = harness.events(true).await;
        assert!(
            header.starts_with("HTTP/1.1 200 OK"),
            "stalled stream retained its permit: {header}"
        );
        readers.push(reader);
    }
    let began = Instant::now();
    harness.finish().await.unwrap();
    assert!(began.elapsed() < Duration::from_secs(1));
    publisher.abort();
}

#[tokio::test(flavor = "current_thread")]
async fn idle_heartbeats_keep_the_stream_open_without_republishing_metrics() {
    let harness = Harness::new("stdin".into());
    let (header, mut reader) = harness.events(false).await;
    assert!(header.contains("cache-control: no-store\r\n"));
    let initial = next_data(&mut reader).await;
    assert_eq!(initial["sequence"], "0");
    timeout(HEARTBEAT + Duration::from_secs(1), async {
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).await.unwrap() > 0);
            assert!(
                !line.starts_with("data: "),
                "idle metrics were serialized again"
            );
            if line == ": heartbeat\n" {
                break;
            }
        }
    })
    .await
    .unwrap();
    let response = harness.get("/api/snapshot").await;
    let current: serde_json::Value =
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(current, initial);
    harness.finish().await.unwrap();
}

#[test]
fn serves_current_frames_and_checks_local_requests() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let server = Server::bind(0).unwrap();
            let address = server.address().unwrap();
            assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
            let slot = Arc::new(SnapshotSlot::new(Stats::new(60, "stdin".into()).snapshot(
                Duration::ZERO,
                InputState::Reading,
                0,
                None,
            )));
            let stop = Arc::new(AtomicUsize::new(0));
            let task = tokio::spawn(server.serve(SnapshotReader { slot }, stop.clone()));
            for (method, path, host, headers, expected) in [
                ("GET", "/api/snapshot", address.to_string(), "", "200 OK"),
                ("GET", "/", address.to_string(), "", "200 OK"),
                ("GET", "/missing", address.to_string(), "", "404 Not Found"),
                (
                    "GET",
                    "/api/snapshot",
                    "foreign.test".into(),
                    "",
                    "403 Forbidden",
                ),
                (
                    "GET",
                    "/api/snapshot",
                    address.to_string(),
                    "Origin: https://foreign.test\r\n",
                    "403 Forbidden",
                ),
                (
                    "POST",
                    "/api/snapshot",
                    address.to_string(),
                    "",
                    "405 Method Not Allowed",
                ),
                (
                    "HEAD",
                    "/api/snapshot",
                    address.to_string(),
                    "",
                    "405 Method Not Allowed",
                ),
                (
                    "GET",
                    "/api/snapshot",
                    address.to_string(),
                    "Content-Length: 1\r\n",
                    "400 Bad Request",
                ),
            ] {
                let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
                socket
                    .write_all(
                        format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n{headers}\r\n")
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
                let mut response = String::new();
                socket.read_to_string(&mut response).await.unwrap();
                assert!(
                    response.starts_with(&format!("HTTP/1.1 {expected}")),
                    "{response}"
                );
                assert!(response.contains("cache-control: no-store\r\n"));
                assert!(response.contains("frame-ancestors 'none'"));
                if expected == "405 Method Not Allowed" {
                    assert!(response.contains("allow: GET\r\n"));
                }
                if path == "/api/snapshot" && expected == "200 OK" {
                    let frame: serde_json::Value =
                        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
                    assert_eq!(frame["web_schema_version"], 1);
                    assert_eq!(frame["sequence"], "0");
                }
            }
            stop.store(1, Ordering::Relaxed);
            task.await.unwrap().unwrap();
        });
}

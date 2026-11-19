//! Authenticated loopback OTLP HTTP/protobuf intake; no raw payload storage.
use super::{
    live::Shared,
    otlp::{self, Batch},
};
use axum::{
    Router,
    body::to_bytes,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::{sync::Semaphore, task::JoinSet};
const TIMEOUT: Duration = Duration::from_secs(2);
#[derive(Clone)]
struct App {
    shared: Shared,
    host: String,
    token: String,
    epoch: String,
    queue: SyncSender<Batch>,
    queued_events: Arc<AtomicUsize>,
}
pub struct Running {
    worker: Option<JoinHandle<()>>,
    stop: Arc<AtomicUsize>,
    pub port: u16,
}
pub(super) fn token(path: &Path) -> io::Result<String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut f) => {
            let mut bytes = [0; 32];
            File::open("/dev/urandom")?.read_exact(&mut bytes)?;
            let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            writeln!(f, "{key}")?;
            return Ok(key);
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => crate::platform::open_regular(path)?,
        Err(e) => return Err(e),
    };
    if file.metadata()?.permissions().mode() & 0o077 != 0 {
        return Err(io::Error::other(
            "Trace credential file must be private (chmod 600)",
        ));
    }
    let mut bytes = Vec::new();
    (&mut file).take(66).read_to_end(&mut bytes)?;
    let key = std::str::from_utf8(&bytes)
        .map_err(|_| io::Error::other("Invalid trace credential file"))?
        .trim();
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::other("Invalid trace credential file"));
    }
    Ok(key.to_owned())
}
impl Running {
    pub fn start(
        port: u16,
        key_path: &Path,
        config_out: Option<&Path>,
        shared: Shared,
        stop: Arc<AtomicUsize>,
    ) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let host = listener.local_addr()?.to_string();
        let port = listener.local_addr()?.port();
        let token = token(key_path)?;
        if let Some(path) = config_out {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            writeln!(
                f,
                "[otel]\nlog_user_prompt = false\nlog_agent_responses = false\nexporter = {{ otlp-http = {{ endpoint = \"http://{host}/v1/logs\", protocol = \"binary\", headers = {{ \"x-tell-tail-probe-key\" = \"{token}\" }} }} }}"
            )?;
        }
        let (tx, rx) = mpsc::sync_channel::<Batch>(4);
        let queue_state = shared.clone();
        let queued_events = Arc::new(AtomicUsize::new(0));
        let consumer_events = queued_events.clone();
        let consumer = thread::Builder::new()
            .name("tell-tail-trace-store".into())
            .spawn(move || {
                while let Ok(batch) = rx.recv() {
                    let count = batch.observations.len();
                    let Ok(mut s) = queue_state.lock() else { break };
                    s.accepted_batches += 1;
                    s.ignored_records += batch.ignored as u64;
                    let now = crate::mcp::time::now_ms();
                    s.last_received_ms = Some(now);
                    for o in batch.observations {
                        if s.store.accept(o, now, false).is_err() {
                            s.rejected_records += 1;
                        }
                    }
                    consumer_events.fetch_sub(count, Ordering::Relaxed);
                }
            })?;
        let epoch = shared
            .lock()
            .map_err(|_| io::Error::other("Trace state failed"))?
            .store
            .snapshot()
            .collector_epoch;
        let app = App {
            shared: shared.clone(),
            host: host.clone(),
            token,
            epoch,
            queue: tx,
            queued_events,
        };
        let cancel = stop.clone();
        {
            let mut s = shared
                .lock()
                .map_err(|_| io::Error::other("Trace state failed"))?;
            s.enabled = true;
            s.running = true;
        }
        let worker = thread::Builder::new()
            .name("tell-tail-trace-http".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .and_then(|rt| rt.block_on(serve(listener, app, cancel)));
                let _ = consumer.join();
                if let Ok(mut s) = shared.lock() {
                    s.running = false;
                    if result.is_err() {
                        s.rejected_batches += 1;
                    }
                }
            })?;
        crate::writer::diagnostic(&format!(
            "tell-tail: trace intake http://{host}/v1/logs · result-only · in-memory\n"
        ));
        Ok(Self {
            worker: Some(worker),
            stop,
            port,
        })
    }
    pub fn join(&mut self) -> io::Result<()> {
        self.worker.take().map_or(Ok(()), |w| {
            w.join()
                .map_err(|_| io::Error::other("Trace receiver failed"))
        })
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self
            .stop
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
        let _ = self.join();
    }
}
fn request_status(req: &Request, app: &App) -> Option<StatusCode> {
    let h = req.headers();
    if req.uri().authority().is_some()
        || req.uri().scheme().is_some()
        || req.uri().query().is_some()
    {
        return Some(StatusCode::BAD_REQUEST);
    }
    if h.contains_key(header::ORIGIN) {
        return Some(StatusCode::FORBIDDEN);
    }
    for name in [
        "host",
        "content-length",
        "content-type",
        "x-tell-tail-probe-key",
    ] {
        if h.get_all(name).iter().count() != 1 {
            return Some(StatusCode::BAD_REQUEST);
        }
    }
    if h[header::HOST] != app.host {
        return Some(StatusCode::FORBIDDEN);
    }
    let supplied = h["x-tell-tail-probe-key"].as_bytes();
    let expected = app.token.as_bytes();
    if supplied.len() != expected.len()
        || supplied
            .iter()
            .zip(expected)
            .fold(0u8, |a, (b, c)| a | (b ^ c))
            != 0
    {
        return Some(StatusCode::FORBIDDEN);
    }
    if h.contains_key(header::TRANSFER_ENCODING)
        || h.contains_key(header::CONTENT_ENCODING)
        || h[header::CONTENT_TYPE] != "application/x-protobuf"
    {
        return Some(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let length = h[header::CONTENT_LENGTH]
        .to_str()
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    if length.is_none() {
        return Some(StatusCode::BAD_REQUEST);
    }
    if length.is_some_and(|n| n > otlp::MAX_BODY) {
        return Some(StatusCode::PAYLOAD_TOO_LARGE);
    }
    None
}
async fn intake(State(app): State<App>, req: Request) -> Response {
    let result = async {
        if let Some(status) = request_status(&req, &app) {
            if let Ok(mut s) = app.shared.lock() {
                s.last_rejection = Some(match status {
                    StatusCode::PAYLOAD_TOO_LARGE => "body_limit",
                    StatusCode::FORBIDDEN => "access_denied",
                    StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_encoding",
                    _ => "request_headers",
                });
            }
            return status;
        }
        let body =
            match tokio::time::timeout(TIMEOUT, to_bytes(req.into_body(), otlp::MAX_BODY)).await {
                Ok(Ok(b)) => b,
                _ => return StatusCode::BAD_REQUEST,
            };
        let batch = match otlp::decode(&body, &app.epoch) {
            Ok(b) => b,
            Err(e) => {
                if let Ok(mut s) = app.shared.lock() {
                    s.last_rejection = Some(e.0);
                }
                return StatusCode::BAD_REQUEST;
            }
        };
        let count = batch.observations.len();
        if app
            .queued_events
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                n.checked_add(count).filter(|n| *n <= 1024)
            })
            .is_err()
        {
            if let Ok(mut s) = app.shared.lock() {
                s.last_rejection = Some("queue_full");
            }
            return StatusCode::SERVICE_UNAVAILABLE;
        }
        if app.queue.try_send(batch).is_err() {
            app.queued_events.fetch_sub(count, Ordering::Relaxed);
            if let Ok(mut s) = app.shared.lock() {
                s.last_rejection = Some("queue_full");
            }
            return StatusCode::SERVICE_UNAVAILABLE;
        }
        StatusCode::OK
    }
    .await;
    if result != StatusCode::OK {
        if let Ok(mut s) = app.shared.lock() {
            s.rejected_batches += 1;
        }
    }
    (
        result,
        [
            (header::CONTENT_TYPE, "application/x-protobuf"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        "",
    )
        .into_response()
}
async fn serve(listener: TcpListener, app: App, stop: Arc<AtomicUsize>) -> io::Result<()> {
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let router = Router::new()
        .route("/v1/logs", post(intake))
        .with_state(app);
    let slots = Arc::new(Semaphore::new(4));
    let mut tasks = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    while stop.load(Ordering::Relaxed) == 0 {
        tokio::select! {
            _=tick.tick()=>(),
            _=tasks.join_next(),if !tasks.is_empty()=>(),
            result=listener.accept()=>{let(socket,_)=result?;let Ok(permit)=slots.clone().try_acquire_owned()else{continue};let service=TowerToHyperService::new(router.clone());tasks.spawn(async move{let _permit=permit;let conn=http1::Builder::new().timer(TokioTimer::new()).keep_alive(false).max_headers(32).max_buf_size(8192).header_read_timeout(TIMEOUT).serve_connection(TokioIo::new(socket),service);let _=tokio::time::timeout(TIMEOUT,conn).await;});}
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpStream, sync::Mutex};
    fn send(port: u16, key: &str, body: &[u8], extra: &str) -> String {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(s,"POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nContent-Type: application/x-protobuf\r\nx-tell-tail-probe-key: {key}\r\n{extra}\r\n",body.len()).unwrap();
        s.write_all(body).unwrap();
        let mut bytes = Vec::new();
        s.read_to_end(&mut bytes).unwrap();
        String::from_utf8(bytes).unwrap()
    }
    #[test]
    fn real_receiver_enforces_access_and_ingests_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        let config = dir.path().join("otel.toml");
        let state = Arc::new(Mutex::new(super::super::live::State::default()));
        let stop = Arc::new(AtomicUsize::new(0));
        let mut running =
            Running::start(0, &path, Some(&config), state.clone(), stop.clone()).unwrap();
        let key = std::fs::read_to_string(&path).unwrap();
        let key = key.trim();
        for (extra, credential, status) in [
            ("Origin: http://127.0.0.1\r\n", key, "403"),
            ("", "wrong", "403"),
            ("Content-Encoding: gzip\r\n", key, "415"),
        ] {
            assert!(
                send(running.port, credential, b"", extra)
                    .starts_with(&format!("HTTP/1.1 {status}"))
            );
        }
        let body = include_bytes!("../../tests/fixtures/tool-trace/codex-result.pb");
        assert!(send(running.port, key, body, "").starts_with("HTTP/1.1 200"));
        assert!(send(running.port, key, body, "").starts_with("HTTP/1.1 200"));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while state.lock().unwrap().accepted_batches < 2 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let s = state.lock().unwrap();
        assert_eq!(s.store.snapshot().retained_calls, 1);
        assert_eq!(s.accepted_batches, 2);
        assert_eq!(s.rejected_batches, 3);
        let page = s.store.page(&Default::default(), None, None).unwrap();
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("railway"));
        assert!(!json.contains("PRIVATE"));
        drop(s);
        stop.store(1, Ordering::Relaxed);
        running.join().unwrap();
        assert!(!state.lock().unwrap().running);
        assert!(config.exists());
    }
}

#[cfg(test)]
mod saturation_tests {
    use super::*;
    #[tokio::test]
    async fn saturated_event_budget_rejects_without_acknowledging_or_growing_queue() {
        let shared = Arc::new(std::sync::Mutex::new(super::super::live::State::default()));
        let (queue, _rx) = mpsc::sync_channel(4);
        let budget = Arc::new(AtomicUsize::new(1024));
        let app = App {
            shared: shared.clone(),
            host: "127.0.0.1:4318".into(),
            token: "a".repeat(64),
            epoch: "epoch".into(),
            queue,
            queued_events: budget.clone(),
        };
        let body = include_bytes!("../../tests/fixtures/tool-trace/codex-result.pb");
        let req = Request::builder()
            .method("POST")
            .uri("/v1/logs")
            .header("host", "127.0.0.1:4318")
            .header("content-type", "application/x-protobuf")
            .header("content-length", body.len())
            .header("x-tell-tail-probe-key", "a".repeat(64))
            .body(axum::body::Body::from(body.as_slice()))
            .unwrap();
        assert_eq!(
            intake(State(app), req).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(budget.load(Ordering::Relaxed), 1024);
        assert_eq!(shared.lock().unwrap().accepted_batches, 0);
        assert_eq!(shared.lock().unwrap().rejected_batches, 1);
    }
}

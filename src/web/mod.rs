//! Loopback web output, enabled by the `web` Cargo feature.
mod assets;
mod browser;
mod socket;

use crate::{
    snapshots::{PublishedSnapshot, SnapshotReader},
    stats::State as InputState,
    web_frame::WebFrame,
};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::stream;
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{
    convert::Infallible,
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::JoinSet,
    time::{Instant, MissedTickBehavior},
};

const CONNECTIONS: usize = 16;
const STREAMS: usize = 4;
const HEADER_BYTES: usize = 8 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const FINAL_GRACE: Duration = Duration::from_millis(250);
const HEARTBEAT: Duration = Duration::from_secs(15);

/// Start only the web runtime, with the listener ready before input is opened.
pub fn run(args: &crate::cli::Args, signals: &crate::runtime::Signals) -> io::Result<i32> {
    let mut server = Server::bind(args.web_port.unwrap_or(0))?;
    server.open_browser = args.open;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;
    let mut running = crate::runtime::Running::start(args, signals.requested.clone())?;
    let result =
        runtime.block_on(server.serve(running.snapshot_reader(), signals.requested.clone()));
    running.stop();
    // Preserve the actual input failure when the server observed a failed frame.
    running.join()?;
    result?;
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}

struct Delivery {
    json: Bytes,
    event: Bytes,
    state: InputState,
    failed: bool,
}

impl Delivery {
    fn encode(publication: PublishedSnapshot) -> io::Result<Self> {
        let frame =
            WebFrame::encode(publication.generation, &publication.snapshot).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "web frame sequence {} at {:.3}s: {error}",
                        publication.generation, publication.snapshot.elapsed_seconds
                    ),
                )
            })?;
        Ok(Self {
            event: Bytes::from(format!(
                "retry: 500\nevent: snapshot\nid: {}\ndata: {}\n\n",
                frame.event_id(),
                frame.json()
            )),
            json: Bytes::copy_from_slice(frame.json().as_bytes()),
            state: publication.snapshot.state,
            failed: false,
        })
    }

    fn failure() -> Self {
        Self {
            json: Bytes::new(),
            event: Bytes::from_static(
                b"event: failure\ndata: {\"message\":\"web output failed\"}\n\n",
            ),
            state: InputState::Failed,
            failed: true,
        }
    }
}

#[derive(Clone)]
struct App {
    frames: watch::Receiver<Arc<Delivery>>,
    streams: Arc<Semaphore>,
    host: String,
}

async fn snapshot(State(app): State<App>) -> Response {
    let frame = app.frames.borrow().clone();
    if frame.failed {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut response = (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        frame.json.clone(),
    )
        .into_response();
    // EventSource does not expose HTTP errors. A one-shot snapshot fetch lets
    // the dashboard distinguish stream capacity from an unavailable server.
    response.headers_mut().insert(
        "x-tell-tail-streams-available",
        app.streams
            .available_permits()
            .to_string()
            .parse()
            .expect("bounded stream count"),
    );
    response
}

async fn events(State(app): State<App>) -> Response {
    let Ok(permit) = app.streams.try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            "web stream capacity reached\n",
        )
            .into_response();
    };
    let mut heartbeat = tokio::time::interval_at(Instant::now() + HEARTBEAT, HEARTBEAT);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let body = stream::unfold(
        (app.frames, permit, heartbeat, true, false),
        |(mut rx, permit, mut heartbeat, first, done)| async move {
            if done {
                return None;
            }
            if !first {
                tokio::select! {
                    changed = rx.changed() => { if changed.is_err() { return None; } }
                    _ = heartbeat.tick() => {
                        return Some((Ok::<_, Infallible>(Bytes::from_static(b": heartbeat\n\n")), (rx, permit, heartbeat, false, false)));
                    }
                }
            }
            let frame = rx.borrow_and_update().clone();
            Some((
                Ok::<_, Infallible>(frame.event.clone()),
                (rx, permit, heartbeat, false, frame.state.is_final()),
            ))
        },
    );
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
}

fn request_status(request: &Request, host: &str) -> Option<StatusCode> {
    let headers = request.headers();
    // Browsers omit the default HTTP port from Host and serialized Origin.
    let default_http = host == "127.0.0.1:80";
    if headers.get_all(header::HOST).iter().count() != 1
        || headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .is_none_or(|h| h != host && !(default_http && h == "127.0.0.1"))
        || headers.get_all(header::ORIGIN).iter().count() > 1
        || headers.get(header::ORIGIN).is_some_and(|h| {
            h.as_bytes() != format!("http://{host}").as_bytes()
                && !(default_http && h == "http://127.0.0.1")
        })
    {
        return Some(StatusCode::FORBIDDEN);
    }
    if request.method() != Method::GET {
        return Some(StatusCode::METHOD_NOT_ALLOWED);
    }
    if request.uri().authority().is_some()
        || request.uri().scheme().is_some()
        || headers.contains_key(header::TRANSFER_ENCODING)
        || headers
            .get(header::CONTENT_LENGTH)
            .is_some_and(|h| h != "0")
        || headers.contains_key(header::UPGRADE)
    {
        return Some(StatusCode::BAD_REQUEST);
    }
    None
}

async fn local_request(State(app): State<App>, request: Request, next: Next) -> Response {
    let mut response = match request_status(&request, &app.host) {
        Some(status) => status.into_response(),
        None => next.run(request).await,
    };
    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        response
            .headers_mut()
            .insert(header::ALLOW, "GET".parse().expect("static header"));
    }
    for (key, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-resource-policy", "same-origin"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(key, value.parse().expect("static header"));
    }
    response
}

fn router(app: App) -> Router {
    assets::routes()
        .route("/api/snapshot", get(snapshot))
        .route("/api/events", get(events))
        .layer(middleware::from_fn_with_state(app.clone(), local_request))
        .with_state(app)
}

/// A loopback listener bound before ingestion starts. No runtime is needed to bind.
pub struct Server {
    listener: TcpListener,
    open_browser: bool,
    #[cfg(test)]
    probe: Arc<socket::Probe>,
}

impl Server {
    pub fn bind(port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot bind web listener at 127.0.0.1:{port}: {error}"),
            )
        })?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            open_browser: false,
            #[cfg(test)]
            probe: Arc::default(),
        })
    }

    pub fn address(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Run on a Tokio runtime with I/O and time enabled. The caller owns ingestion
    /// and shares its signal flag; EOF retains final state until that flag changes.
    pub async fn serve(self, reader: SnapshotReader, stop: Arc<AtomicUsize>) -> io::Result<()> {
        let host = self.address()?.to_string();
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let mut subscription = reader.subscribe()?;
        let initial = subscription.changed().await.expect("initial publication");
        let (tx, frames) = watch::channel(Arc::new(Delivery::encode(initial)?));
        let app = router(App {
            frames,
            streams: Arc::new(Semaphore::new(STREAMS)),
            host: host.clone(),
        });
        let connections = Arc::new(Semaphore::new(CONNECTIONS));
        let mut tasks = JoinSet::new();
        let mut control = tokio::time::interval(Duration::from_millis(10));
        control.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut open = true;
        let mut completed = false;
        let mut deadline = None;
        let mut failure = None;
        crate::writer::diagnostic(&format!("tell-tail: web http://{host}/ (Ctrl+C to stop)\n"));
        let mut browser = if self.open_browser && stop.load(Ordering::Relaxed) == 0 {
            match browser::Browser::open(&format!("http://{host}/")) {
                Ok(browser) => Some(browser),
                Err(_) => {
                    browser::report_failure();
                    None
                }
            }
        } else {
            None
        };
        loop {
            if browser.as_mut().is_some_and(browser::Browser::failed) {
                browser::report_failure();
            }
            if tx.borrow().state == InputState::Complete && !completed {
                completed = true;
                crate::writer::diagnostic(
                    "tell-tail: input complete; keeping the web server available until Ctrl+C\n",
                );
            }
            if tx.borrow().state == InputState::Failed && failure.is_none() {
                failure = Some(io::Error::other("ingestion failed"));
            }
            if failure.is_some() && deadline.is_none() {
                let _ = stop.compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
                deadline = Some(Instant::now() + FINAL_GRACE);
            }
            if stop.load(Ordering::Relaxed) != 0 && deadline.is_none() {
                deadline = Some(Instant::now() + FINAL_GRACE);
            }
            // A short control tick bridges the existing signal flag, never metric
            // publication. Snapshot updates arrive only through notifications.
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(86400))) => { break; }
                _ = control.tick() => (),
                joined = tasks.join_next(), if !tasks.is_empty() => {
                    if joined.is_some_and(|result| result.is_err()) {
                        failure = Some(io::Error::other("web connection task panicked"));
                        tx.send_replace(Arc::new(Delivery::failure()));
                    }
                }
                publication = subscription.changed(), if open && failure.is_none() => {
                    match publication {
                        Some(publication) => match Delivery::encode(publication) {
                            Ok(frame) => { tx.send_replace(Arc::new(frame)); }
                            Err(error) => {
                                failure = Some(error);
                                open = false;
                                tx.send_replace(Arc::new(Delivery::failure()));
                            }
                        },
                        None => {
                            open = false;
                            if !tx.borrow().state.is_final() {
                                failure = Some(io::Error::other("ingestion ended without a final snapshot"));
                                tx.send_replace(Arc::new(Delivery::failure()));
                            }
                        }
                    }
                }
                accepted = listener.accept(), if deadline.is_none() => {
                    match accepted {
                        Ok((socket, _)) => {
                            let Ok(permit) = connections.clone().try_acquire_owned() else { continue; };
                            if socket.set_nodelay(true).is_err()
                                || rustix::net::sockopt::set_socket_send_buffer_size(&socket, 64 * 1024).is_err()
                            { continue; }
                            let service = TowerToHyperService::new(app.clone());
                            #[cfg(test)]
                            let probe = self.probe.clone();
                            tasks.spawn(async move {
                                let _permit = permit;
                                let io = socket::ProgressIo::new(socket, IO_TIMEOUT);
                                #[cfg(test)]
                                let io = io.with_probe(probe);
                                let io = TokioIo::new(io);
                                let _ = http1::Builder::new()
                                    .timer(TokioTimer::new())
                                    .keep_alive(false)
                                    .half_close(false)
                                    .max_headers(64)
                                    .header_read_timeout(IO_TIMEOUT)
                                    .max_buf_size(HEADER_BYTES)
                                    .writev(false)
                                    .serve_connection(io, service).await;
                            });
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                        Err(error) => {
                            failure = Some(error);
                            tx.send_replace(Arc::new(Delivery::failure()));
                        }
                    }
                }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        failure.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests;

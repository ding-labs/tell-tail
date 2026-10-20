use axum::{
    Json, Router,
    body::Body,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use clap::{Parser, ValueEnum};
use futures_util::stream;
use serde_json::json;
use std::{
    convert::Infallible,
    future::IntoFuture,
    io,
    path::PathBuf,
    sync::{Arc, atomic::AtomicUsize},
    time::{Duration, Instant},
};
use tell_tail::{cli::Args, runtime::Running};
use tell_tail_web_transport::{Frame, Profile, fixture, metrics};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    time::{MissedTickBehavior, timeout},
};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Source {
    Fixture,
    Stdin,
    File,
}

#[derive(Parser)]
#[command(about = "Development-only SSE/WebSocket comparison; not a web output mode")]
struct Config {
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=100))]
    hz: u32,
    #[arg(long, value_enum, default_value = "representative")]
    profile: Profile,
    #[arg(long, value_enum, default_value = "fixture")]
    source: Source,
    #[arg(long)]
    input_file: Option<PathBuf>,
    /// Synthetic padding for undrained-socket stress, never normal measurements.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=60_000))]
    padding_bytes: u32,
}

#[derive(Clone)]
struct App {
    frames: watch::Receiver<Arc<Frame>>,
    clients: Arc<Semaphore>,
    clock: Instant,
    host: String,
}

async fn local_request(
    State(app): State<App>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let valid = request.headers().get("host").and_then(|h| h.to_str().ok()) == Some(&app.host)
        && request
            .headers()
            .get("origin")
            .is_none_or(|h| h.to_str().ok() == Some(&format!("http://{}", app.host)));
    if !valid {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

async fn sse(State(app): State<App>) -> Response {
    let Ok(permit) = app.clients.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let frames = stream::unfold(
        (app.frames, permit, true),
        |(mut rx, permit, first)| async move {
            if !first && rx.changed().await.is_err() {
                return None;
            }
            let frame = rx.borrow_and_update().clone();
            Some((Ok::<_, Infallible>(frame.sse.clone()), (rx, permit, false)))
        },
    );
    (
        [
            ("content-type", "text/event-stream"),
            ("cache-control", "no-store"),
        ],
        Body::from_stream(frames),
    )
        .into_response()
}

async fn websocket(State(app): State<App>, ws: WebSocketUpgrade) -> Response {
    let Ok(permit) = app.clients.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    ws.write_buffer_size(0)
        .max_write_buffer_size(128 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            send_websocket(socket, app.frames).await;
        })
}

async fn send_websocket(mut socket: WebSocket, mut frames: watch::Receiver<Arc<Frame>>) {
    loop {
        let frame = frames.borrow_and_update().clone();
        if !matches!(
            timeout(
                Duration::from_secs(2),
                socket.send(Message::Text(frame.json.clone()))
            )
            .await,
            Ok(Ok(()))
        ) {
            return;
        }
        loop {
            tokio::select! {
                changed = frames.changed() => {
                    if changed.is_err() { return; }
                    break;
                }
                incoming = socket.recv() => {
                    if matches!(incoming, None | Some(Err(_)) | Some(Ok(Message::Close(_)))) { return; }
                }
            }
        }
    }
}

async fn clock_route(State(app): State<App>) -> Json<serde_json::Value> {
    Json(json!({"monotonic_us": app.clock.elapsed().as_micros() as u64}))
}

async fn status(State(app): State<App>) -> Json<serde_json::Value> {
    let frame = app.frames.borrow().clone();
    Json(json!({
        "sequence": frame.sequence, "payload_bytes": frame.json.len(),
        "active_clients": 4 - app.clients.available_permits(),
        "monotonic_us": app.clock.elapsed().as_micros() as u64,
        "frame": serde_json::from_str::<serde_json::Value>(frame.json.as_str()).unwrap(),
    }))
}

fn input(config: &Config) -> io::Result<Option<Running>> {
    let args = match config.source {
        Source::Fixture if config.input_file.is_none() => return Ok(None),
        Source::Stdin if config.input_file.is_none() => Args::parse_from(["tell-tail"]),
        Source::File if config.input_file.is_some() => Args::parse_from([
            std::ffi::OsStr::new("tell-tail"),
            config.input_file.as_ref().unwrap().as_os_str(),
            std::ffi::OsStr::new("--from"),
            std::ffi::OsStr::new("start"),
        ]),
        _ => {
            return Err(io::Error::other(
                "--input-file is required only with --source file",
            ));
        }
    };
    Running::start(&args, Arc::new(AtomicUsize::new(0))).map(Some)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let config = Config::parse();
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port)).await?;
    let host = listener.local_addr()?.to_string();
    let mut running = input(&config)?;
    let captured = fixture(config.profile);
    let clock = Instant::now();
    let initial = running.as_ref().map_or_else(
        || captured.clone(),
        |r| metrics(serde_json::to_value(r.snapshot()).unwrap()),
    );
    let (tx, rx) = watch::channel(Arc::new(Frame::new(0, 0, &initial)));
    let mut publisher = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(config.hz)));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence = 0;
        loop {
            tick.tick().await;
            let mut value = match running.as_mut() {
                Some(running) => metrics(serde_json::to_value(running.snapshot()).unwrap()),
                None => captured.clone(),
            };
            if config.padding_bytes != 0 {
                value["stress_padding"] = "x".repeat(config.padding_bytes as usize).into();
            }
            sequence += 1;
            let frame = Arc::new(Frame::new(
                sequence,
                clock.elapsed().as_micros() as u64,
                &value,
            ));
            assert!(
                frame.json.len() <= 64 * 1024,
                "benchmark frame exceeds 64 KiB"
            );
            tx.send_replace(frame);
        }
    });
    let app = App {
        frames: rx,
        clients: Arc::new(Semaphore::new(4)),
        clock,
        host: host.clone(),
    };
    let router = Router::new()
        .route(
            "/",
            get(|| async { Html("<h1>Tell-Tail transport experiment</h1>") }),
        )
        .route("/events", get(sse))
        .route("/ws", get(websocket))
        .route("/clock", get(clock_route))
        .route("/status", get(status))
        .layer(middleware::from_fn_with_state(app.clone(), local_request))
        .with_state(app);
    println!(
        "{}",
        json!({"url": format!("http://{host}"), "pid": std::process::id(), "hz": config.hz})
    );
    let shutdown = async {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = terminate.recv() => () }
    };
    // Runtime teardown drops connections; no indefinite graceful stream drain.
    tokio::select! {
        result = axum::serve(listener, router).into_future() => { publisher.abort(); result }
        result = &mut publisher => Err(io::Error::other(format!("publisher stopped: {result:?}"))),
        _ = shutdown => { publisher.abort(); Ok(()) }
    }
}

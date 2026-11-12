//! MCP mode shares the existing loopback security and bounded socket transport.
use super::{
    CONNECTIONS, HEADER_BYTES, IO_TIMEOUT, STREAMS, browser, request_status, secure_response,
    socket,
};
use crate::mcp::{
    cli::Args,
    model::Event,
    runtime::{Running, Shared},
};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
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
    net::{Ipv4Addr, TcpListener},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::JoinSet,
    time::MissedTickBehavior,
};

#[derive(Clone)]
struct App {
    shared: Shared,
    frames: watch::Receiver<Bytes>,
    streams: Arc<Semaphore>,
    host: String,
}

pub fn run(args: &Args, signals: &crate::runtime::Signals) -> io::Result<i32> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, args.web_port.unwrap_or(0)))?;
    listener.set_nonblocking(true)?;
    let host = listener.local_addr()?.to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut running = Running::start(args, signals.requested.clone())?;
    let result = runtime.block_on(serve(listener, host, args.open, &mut running, signals));
    running.stop();
    running.join()?;
    result?;
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}
async fn serve(
    listener: TcpListener,
    host: String,
    open: bool,
    running: &mut Running,
    signals: &crate::runtime::Signals,
) -> io::Result<()> {
    let listener = tokio::net::TcpListener::from_std(listener)?;
    let (tx, frames) = watch::channel(Bytes::from(running.snapshot()?.json()?));
    let app = router(App {
        shared: running.shared.clone(),
        frames,
        streams: Arc::new(Semaphore::new(STREAMS)),
        host: host.clone(),
    });
    let slots = Arc::new(Semaphore::new(CONNECTIONS));
    let mut tasks = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    crate::writer::diagnostic(&format!(
        "tell-tail: MCP web http://{host}/ (Ctrl+C to stop)\n"
    ));
    let mut browser = if open {
        match browser::Browser::open(&format!("http://{host}/")) {
            Ok(b) => Some(b),
            Err(_) => {
                browser::report_failure();
                None
            }
        }
    } else {
        None
    };
    let result = loop {
        if signals.requested.load(Ordering::Relaxed) != 0 {
            break Ok(());
        }
        if running.is_finished() {
            break Err(io::Error::other("MCP collector stopped unexpectedly"));
        }
        if browser.as_mut().is_some_and(browser::Browser::failed) {
            browser::report_failure();
        }
        tokio::select! {
            _=tick.tick()=>{
                match running.snapshot().and_then(|s|s.json().map_err(io::Error::other)){
                    Ok(bytes)=>{tx.send_replace(Bytes::from(bytes));}
                    Err(e)=>break Err(e),
                }
            }
            joined=tasks.join_next(),if !tasks.is_empty()=>{if joined.is_some_and(|r|r.is_err()){break Err(io::Error::other("MCP web connection failed"));}}
            accepted=listener.accept()=>{
                let (socket,_)=match accepted{Ok(pair)=>pair,Err(e) if e.kind()==io::ErrorKind::Interrupted=>continue,Err(e)=>break Err(e)};
                let Ok(permit)=slots.clone().try_acquire_owned() else{continue;};
                if socket.set_nodelay(true).is_err() || rustix::net::sockopt::set_socket_send_buffer_size(&socket,64*1024).is_err(){continue;}
                let service=TowerToHyperService::new(app.clone());
                tasks.spawn(async move{
                    let _permit=permit;
                    let io=TokioIo::new(socket::ProgressIo::new(socket,IO_TIMEOUT));
                    let _=http1::Builder::new().timer(TokioTimer::new()).keep_alive(false).half_close(false).max_headers(64).header_read_timeout(IO_TIMEOUT).max_buf_size(HEADER_BYTES).writev(false).serve_connection(io,service).await;
                });
            }
        }
    };
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    result
}
async fn local(State(app): State<App>, request: Request, next: Next) -> Response {
    secure_response(match request_status(&request, &app.host) {
        Some(s) => s.into_response(),
        None => next.run(request).await,
    })
}
async fn snapshot(State(app): State<App>) -> Response {
    let bytes = app.frames.borrow().clone();
    let mut r = ([(header::CONTENT_TYPE, "application/json")], bytes).into_response();
    r.headers_mut().insert(
        "x-tell-tail-streams-available",
        app.streams.available_permits().to_string().parse().unwrap(),
    );
    r
}
async fn events(State(app): State<App>) -> Response {
    let Ok(permit) = app.streams.try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            "Stream capacity reached",
        )
            .into_response();
    };
    let body = stream::unfold(
        (app.frames, permit, true),
        |(mut rx, permit, first)| async move {
            if !first && rx.changed().await.is_err() {
                return None;
            }
            let json = rx.borrow_and_update().clone();
            let mut bytes = Vec::with_capacity(json.len() + 40);
            bytes.extend_from_slice(b"retry: 1000\nevent: snapshot\ndata: ");
            bytes.extend_from_slice(&json);
            bytes.extend_from_slice(b"\n\n");
            Some((Ok::<_, Infallible>(Bytes::from(bytes)), (rx, permit, false)))
        },
    );
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
}
async fn detail(State(app): State<App>, Path(id): Path<u64>) -> Response {
    let event = match app.shared.lock() {
        Ok(s) => s.events.iter().find(|e| e.id == id).cloned(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(event) = event else {
        return (StatusCode::GONE, "Evidence has left the bounded history").into_response();
    };
    json_response(&event, 16 * 1024)
}
fn json_response(value: &impl serde::Serialize, limit: usize) -> Response {
    match serde_json::to_vec(value) {
        Ok(b) if b.len() <= limit => {
            ([(header::CONTENT_TYPE, "application/json")], b).into_response()
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
fn decode(s: &str) -> Option<String> {
    let mut out = Vec::new();
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        out.push(match b {
            b'%' => {
                let a = (bytes.next()? as char).to_digit(16)?;
                let b = (bytes.next()? as char).to_digit(16)?;
                (a * 16 + b) as u8
            }
            b'+' => b' ',
            _ => b,
        });
        if out.len() > 256 {
            return None;
        }
    }
    String::from_utf8(out).ok()
}
async fn history(State(app): State<App>, request: Request) -> Response {
    let query = request.uri().query().unwrap_or("");
    if query.len() > 2048 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut filters = std::collections::BTreeMap::new();
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let Some((k, v)) = pair.split_once('=') else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if !matches!(
            k,
            "q" | "host" | "server" | "category" | "since" | "before" | "until"
        ) {
            return StatusCode::BAD_REQUEST.into_response();
        }
        let Some(v) = decode(v) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if filters.insert(k, v).is_some() {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }
    let before = match filters.get("before").map(|v| v.parse::<u64>()).transpose() {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let since = match filters.get("since").map(|v| v.parse::<i64>()).transpose() {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let until = match filters.get("until").map(|v| v.parse::<u64>()).transpose() {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let s = match app.shared.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let key = |e: &Event| (e.timestamp_ms.unwrap_or(e.observed_ms), e.id);
    let cursor = before.and_then(|id| s.events.iter().find(|e| e.id == id).map(key));
    if before.is_some() && cursor.is_none() {
        return (StatusCode::GONE, "History cursor expired; refresh the view").into_response();
    }
    let mut rows: Vec<_> = s
        .events
        .iter()
        .filter(|e| {
            if cursor.is_some_and(|c| key(e) >= c)
                || since.is_some_and(|t| key(e).0 < t)
                || until.is_some_and(|id| e.id > id)
            {
                return false;
            }
            if let Some(host) = filters.get("host")
                && !host.is_empty()
                && serde_json::to_value(e.host)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .as_ref()
                    != Some(host)
            {
                return false;
            }
            if let Some(server) = filters.get("server")
                && !server.is_empty()
                && &e.server != server
            {
                return false;
            }
            if let Some(cat) = filters.get("category")
                && !cat.is_empty()
            {
                if cat == "failure" {
                    if !e.kind.is_failure() {
                        return false;
                    }
                } else if serde_json::to_value(e.reason)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .as_ref()
                    != Some(cat)
                {
                    return false;
                }
            }
            filters.get("q").is_none_or(|q| {
                format!(
                    "{} {} {} {}",
                    e.host.label(),
                    e.server,
                    e.summary,
                    e.method.unwrap_or("")
                )
                .to_lowercase()
                .contains(&q.to_lowercase())
            })
        })
        .cloned()
        .collect();
    rows.sort_by_key(|e| std::cmp::Reverse(key(e)));
    let matched = rows.len();
    rows.truncate(100);
    let retained = s.events.len();
    drop(s);
    while serde_json::to_vec(&rows).is_ok_and(|b| b.len() > 60 * 1024) {
        rows.pop();
    }
    json_response(
        &serde_json::json!({"schema_version":1,"events":rows,"matched":matched,"retained":retained,"next_before":if matched>rows.len(){rows.last().map(|e|e.id)}else{None}}),
        64 * 1024,
    )
}
fn router(app: App) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../../web/mcp.html")) }),
        )
        .route(
            "/dashboard.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../../web/dashboard.css"),
                )
            }),
        )
        .route(
            "/mcp.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../../web/mcp.css"),
                )
            }),
        )
        .route(
            "/mcp.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/mcp.mjs"),
                )
            }),
        )
        .route(
            "/logo.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!("../../web/logo.svg"),
                )
            }),
        )
        .route(
            "/favicon.svg",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "image/svg+xml")],
                    include_str!("../../web/favicon.svg"),
                )
            }),
        )
        .route("/api/mcp/snapshot", get(snapshot))
        .route("/api/mcp/events", get(events))
        .route("/api/mcp/event/{id}", get(detail))
        .route("/api/mcp/history", get(history))
        .layer(middleware::from_fn_with_state(app.clone(), local))
        .with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_decoding_is_bounded_and_validates_utf8() {
        assert_eq!(decode("a%20b%2Bc").as_deref(), Some("a b+c"));
        assert!(decode("%ff").is_none());
        assert!(decode("%2").is_none());
        assert!(decode(&"x".repeat(257)).is_none());
    }
}

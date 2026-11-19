//! Read-only trace queries and coalesced watermark notifications.
use crate::tool_trace::{live::Shared, model::Status, store::Filter};
use axum::{
    Router,
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::stream;
use std::{collections::BTreeMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
#[derive(Clone)]
struct App {
    shared: Shared,
    streams: Arc<Semaphore>,
}
fn json(value: &impl serde::Serialize) -> Response {
    match serde_json::to_vec(value) {
        Ok(b) if b.len() <= 64 * 1024 => {
            ([(header::CONTENT_TYPE, "application/json")], b).into_response()
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
fn query(req: &Request, allowed: &[&str]) -> Result<BTreeMap<String, String>, StatusCode> {
    let q = req.uri().query().unwrap_or("");
    if q.len() > 2048 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut params = BTreeMap::new();
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').ok_or(StatusCode::BAD_REQUEST)?;
        if !allowed.contains(&k) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let v = super::mcp::decode(v).ok_or(StatusCode::BAD_REQUEST)?;
        if params.insert(k.to_owned(), v).is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    Ok(params)
}
fn integer(p: &BTreeMap<String, String>, key: &str) -> Result<Option<u64>, StatusCode> {
    p.get(key)
        .map(|v| v.parse().map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()
}
fn failure(e: &str) -> Response {
    let status = if e.contains("expired") || e.contains("unavailable") {
        StatusCode::GONE
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, e.to_owned()).into_response()
}
async fn snapshot(State(app): State<App>) -> Response {
    match app.shared.lock() {
        Ok(s) => json(&s.snapshot()),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
async fn calls(State(app): State<App>, req: Request) -> Response {
    let p = match query(
        &req,
        &[
            "plugin",
            "tool",
            "status",
            "execution",
            "q",
            "since",
            "as_of",
            "cursor",
            "epoch",
        ],
    ) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let as_of = match integer(&p, "as_of") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let since = match integer(&p, "since") {
        Ok(v) => match v.map(i64::try_from).transpose() {
            Ok(v) => v,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        },
        Err(e) => return e.into_response(),
    };
    let status = match p.get("status").map(String::as_str) {
        None | Some("") => None,
        Some("running") => Some(Status::Running),
        Some("completed") => Some(Status::Completed),
        Some("failed") => Some(Status::Failed),
        Some("cancelled") => Some(Status::Cancelled),
        Some("denied") => Some(Status::Denied),
        Some("incomplete") => Some(Status::Incomplete),
        Some("conflict") => Some(Status::Conflict),
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let f = Filter {
        plugin: p.get("plugin").cloned(),
        tool: p.get("tool").cloned(),
        status,
        execution: p.get("execution").cloned(),
        search: p.get("q").cloned(),
        since_ms: since,
    };
    let s = match app.shared.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if p.get("epoch")
        .is_some_and(|e| e != &s.store.snapshot().collector_epoch)
    {
        return (
            StatusCode::GONE,
            "Collector restarted; resume to load current calls",
        )
            .into_response();
    }
    match s.store.page(&f, as_of, p.get("cursor").map(String::as_str)) {
        Ok(page) => json(&page),
        Err(e) => failure(e),
    }
}
async fn detail(State(app): State<App>, Path(id): Path<u64>, req: Request) -> Response {
    let p = match query(&req, &["as_of", "epoch"]) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let as_of = match integer(&p, "as_of") {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let s = match app.shared.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let snap = s.store.snapshot();
    if p.get("epoch").is_some_and(|e| e != &snap.collector_epoch) {
        return StatusCode::GONE.into_response();
    }
    match s.store.detail(id, as_of.unwrap_or(snap.watermark)) {
        Ok(call) => json(&call),
        Err(e) => failure(e),
    }
}
async fn events(State(app): State<App>) -> Response {
    let Ok(permit) = app.streams.try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let body = stream::unfold(
        (
            app.shared,
            permit,
            String::new(),
            tokio::time::Instant::now(),
        ),
        |(shared, permit, mut last, mut sent)| async move {
            loop {
                let current = shared
                    .lock()
                    .ok()
                    .and_then(|s| serde_json::to_string(&s.snapshot()).ok())?;
                if current != last {
                    last = current;
                    sent = tokio::time::Instant::now();
                    let frame = format!("retry: 1000\nevent: trace_source\ndata: {last}\n\n");
                    return Some((Ok::<_, Infallible>(frame), (shared, permit, last, sent)));
                }
                if sent.elapsed() > Duration::from_secs(15) {
                    sent = tokio::time::Instant::now();
                    return Some((Ok(": keep-alive\n\n".into()), (shared, permit, last, sent)));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        },
    );
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
}
pub(super) fn router(shared: Shared) -> Router {
    Router::new()
        .route("/api/traces/snapshot", get(snapshot))
        .route("/api/traces/calls", get(calls))
        .route("/api/traces/call/{id}", get(detail))
        .route("/api/traces/events", get(events))
        .with_state(App {
            shared,
            streams: Arc::new(Semaphore::new(super::STREAMS)),
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_rejects_unknown_duplicate_oversized_values() {
        for q in ["unknown=x", "q=a&q=b", "as_of=%ff", "q=%", "status=x&bad=y"] {
            let r = Request::builder()
                .uri(format!("/api/traces/calls?{q}"))
                .body(Body::empty())
                .unwrap();
            assert!(query(&r, &["q", "as_of", "status"]).is_err());
        }
    }
}

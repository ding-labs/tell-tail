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
    turn_reads: Arc<Semaphore>,
}
fn json(value: &impl serde::Serialize) -> Response {
    json_bounded(value, 64 * 1024)
}
fn json_bounded(value: &impl serde::Serialize, limit: usize) -> Response {
    match serde_json::to_vec(value) {
        Ok(b) if b.len() <= limit => {
            ([(header::CONTENT_TYPE, "application/json")], b).into_response()
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
pub(super) fn query(
    req: &Request,
    allowed: &[&str],
) -> Result<BTreeMap<String, String>, StatusCode> {
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
pub(super) fn private_input(
    req: &Request,
    context: Option<&crate::tool_trace::inspection::Inspection>,
) -> Response {
    if req
        .headers()
        .get("x-tell-tail-inspect")
        .and_then(|v| v.to_str().ok())
        != Some("local-input")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    match context.and_then(|c| c.input.as_deref()) {
        Some(input) => json(&serde_json::json!({"input":input})),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn detail(State(app): State<App>, Path(id): Path<u64>, req: Request) -> Response {
    let p = match query(&req, &["as_of", "epoch", "input"]) {
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
        Ok(call) if p.contains_key("input") => private_input(&req, call.inspection.as_deref()),
        Ok(call) => json(&call),
        Err(e) => failure(e),
    }
}
async fn turn(State(app): State<App>, Path(id): Path<u64>, req: Request) -> Response {
    if req
        .headers()
        .get("x-tell-tail-inspect")
        .and_then(|v| v.to_str().ok())
        != Some("local-turn")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(params) = query(&req, &["source"]) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(permit) = app.turn_reads.try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let (root, locator, live, scope) = {
        let Ok(state) = app.shared.lock() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let Some(root) = state.turn_root.clone() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let Ok(call) = state.store.detail(id, state.store.snapshot().watermark) else {
            return StatusCode::GONE.into_response();
        };
        if params.get("source") != Some(&call.source) {
            return StatusCode::GONE.into_response();
        }
        let Some(locator) = call.inspection.and_then(|c| c.locator.clone()) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let live = state.store.chat_tools(&locator.chat);
        let scope = state.links.resolve(&locator, &live);
        (root, locator, live, scope)
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::tool_trace::turn::read(&root, &locator, &scope, &live)
    })
    .await;
    match result {
        Ok(Ok(evidence)) => json_bounded(&evidence, 512 * 1024),
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
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
                tokio::time::sleep(Duration::from_millis(200)).await;
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
        .route(
            "/call-input.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/call-input.mjs"),
                )
            }),
        )
        .route("/api/traces/snapshot", get(snapshot))
        .route("/api/traces/calls", get(calls))
        .route("/api/traces/call/{id}", get(detail))
        .route("/api/traces/call/{id}/turn", get(turn))
        .route("/api/traces/events", get(events))
        .with_state(App {
            shared,
            streams: Arc::new(Semaphore::new(super::STREAMS)),
            turn_reads: Arc::new(Semaphore::new(2)),
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

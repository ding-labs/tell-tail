//! A single watermark binds activity rows, bins, source state and inspection.
use crate::activity::{
    model::Category,
    runtime::Shared,
    store::{
        Store,
        query::{Filter, View},
    },
};
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
const QUERY: &[&str] = &[
    "epoch",
    "as_of",
    "cursor",
    "view",
    "category",
    "q",
    "plugin",
    "server",
    "source",
    "client",
    "scope",
    "outcome",
    "historical",
    "since",
    "until",
    "diagnostics",
];
fn json(value: &impl serde::Serialize, limit: usize) -> Response {
    match serde_json::to_vec(value) {
        Ok(bytes) if bytes.len() <= limit => {
            ([(header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Activity response exceeds limit",
        )
            .into_response(),
    }
}
fn failure(e: &'static str) -> Response {
    (
        if e.contains("expired") || e.contains("unavailable") || e.contains("restarted") {
            StatusCode::GONE
        } else {
            StatusCode::BAD_REQUEST
        },
        e,
    )
        .into_response()
}
fn boolean(p: &BTreeMap<String, String>, key: &str) -> Result<Option<bool>, &'static str> {
    match p.get(key).map(String::as_str) {
        None => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        _ => Err("Invalid boolean filter"),
    }
}
fn time(p: &BTreeMap<String, String>, key: &str) -> Result<Option<i64>, &'static str> {
    p.get(key)
        .map(|v| {
            v.parse::<i64>()
                .ok()
                .filter(|t| (0..=253402300799999).contains(t))
                .ok_or("Invalid time filter")
        })
        .transpose()
}
fn filters(p: &BTreeMap<String, String>) -> Result<Filter, &'static str> {
    let optional = |key| p.get(key).filter(|v| !v.is_empty()).cloned();
    let category = match p.get("category").map(String::as_str) {
        None | Some("") => None,
        Some("tools") => Some(Category::Tools),
        Some("mcp") => Some(Category::Mcp),
        Some("connections") => Some(Category::Connections),
        Some("desktop") => Some(Category::Desktop),
        _ => return Err("Invalid activity category"),
    };
    let view = match p.get("view").map(String::as_str) {
        None | Some("activity") => View::Activity,
        Some("tools") => View::Tools,
        Some("problems") => View::Problems,
        _ => return Err("Invalid activity view"),
    };
    let since_ms = time(p, "since")?;
    let until_ms = time(p, "until")?;
    if since_ms.zip(until_ms).is_some_and(|(a, b)| a >= b) {
        return Err("Invalid activity time range");
    }
    let outcome = optional("outcome");
    if outcome.as_deref().is_some_and(|v| {
        ![
            "observed",
            "opened_observed",
            "closed_observed",
            "retry_scheduled",
            "ready_observed",
            "start_observed",
            "completed",
            "failure_reported",
            "execution_succeeded",
            "execution_failed",
            "command_failed",
            "tool_error",
            "cancelled",
            "denied",
            "incomplete",
            "conflicting_evidence",
        ]
        .contains(&v)
    }) {
        return Err("Invalid outcome filter");
    }
    Ok(Filter {
        view,
        category,
        search: p.get("q").cloned().unwrap_or_default().to_lowercase(),
        plugin: optional("plugin"),
        server: optional("server"),
        source: optional("source"),
        client: optional("client"),
        scope: optional("scope"),
        outcome,
        historical: boolean(p, "historical")?,
        since_ms,
        until_ms,
        diagnostics: boolean(p, "diagnostics")?.unwrap_or(false),
    })
}
fn watermark(s: &Store, p: &BTreeMap<String, String>) -> Result<u64, &'static str> {
    if p.get("epoch").is_some_and(|v| v != &s.epoch) {
        return Err("Collector restarted; return to live activity");
    }
    let at = p
        .get("as_of")
        .map(|v| v.parse().map_err(|_| "Invalid activity watermark"))
        .transpose()?
        .unwrap_or(s.sequence);
    s.check(at)?;
    Ok(at)
}
async fn snapshot(State(app): State<App>, req: Request) -> Response {
    if let Err(e) = super::trace::query(&req, &[]) {
        return e.into_response();
    }
    match app.shared.lock() {
        Ok(s) => json(&s.snapshot(), 64 * 1024),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
enum Query {
    Items,
    Bins,
    Problems,
    Sources,
}
fn read(app: &App, req: &Request, kind: Query) -> Response {
    let p = match super::trace::query(req, QUERY) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let f = match filters(&p) {
        Ok(f) => f,
        Err(e) => return failure(e),
    };
    let s = match app.shared.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let at = match watermark(&s, &p) {
        Ok(at) => at,
        Err(e) => return failure(e),
    };
    match kind {
        Query::Items => match s.page(&f, at, p.get("cursor").map(String::as_str)) {
            Ok(v) => json(&v, 64 * 1024),
            Err(e) => failure(e),
        },
        Query::Bins => match s.bins(&f, at, 60) {
            Ok(v) => json(&v, 64 * 1024),
            Err(e) => failure(e),
        },
        Query::Problems => match s.problems(&f, at) {
            Ok(mut groups) => {
                let matched = groups.len();
                groups.truncate(100);
                while serde_json::to_vec(&groups).is_ok_and(|v| v.len() > 48 * 1024) {
                    groups.pop();
                }
                json(
                    &serde_json::json!({"as_of":at.to_string(),"matched":matched,"omitted":matched.saturating_sub(groups.len()),"groups":groups}),
                    64 * 1024,
                )
            }
            Err(e) => failure(e),
        },
        Query::Sources => match s.sources_at(at) {
            Ok(sources) => json(
                &serde_json::json!({"as_of":at.to_string(),"sources":sources}),
                64 * 1024,
            ),
            Err(e) => failure(e),
        },
    }
}
async fn items(State(app): State<App>, req: Request) -> Response {
    read(&app, &req, Query::Items)
}
async fn bins(State(app): State<App>, req: Request) -> Response {
    read(&app, &req, Query::Bins)
}
async fn problems(State(app): State<App>, req: Request) -> Response {
    read(&app, &req, Query::Problems)
}
async fn sources(State(app): State<App>, req: Request) -> Response {
    read(&app, &req, Query::Sources)
}
async fn detail(State(app): State<App>, Path(id): Path<u64>, req: Request) -> Response {
    let p = match super::trace::query(&req, &["epoch", "as_of", "input"]) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let s = match app.shared.lock() {
        Ok(s) => s,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match watermark(&s, &p).and_then(|at| s.detail(id, at)) {
        Ok(item) if p.contains_key("input") => {
            super::trace::private_input(&req, item.inspection.as_deref())
        }
        Ok(item) => json(&item, 16 * 1024),
        Err(e) => failure(e),
    }
}
async fn events(State(app): State<App>, req: Request) -> Response {
    if let Err(e) = super::trace::query(&req, &[]) {
        return e.into_response();
    }
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
                let notification = shared.lock().ok().map(|s| serde_json::json!({"collector_epoch":s.epoch,"watermark":s.sequence.to_string(),"oldest_watermark":s.floor.to_string()}).to_string())?;
                if notification != last {
                    last = notification;
                    sent = tokio::time::Instant::now();
                    return Some((
                        Ok::<_, Infallible>(format!(
                            "retry: 1000\nevent: activity\ndata: {last}\n\n"
                        )),
                        (shared, permit, last, sent),
                    ));
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
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}
pub(super) fn router(shared: Shared) -> Router {
    Router::new()
        .route(
            "/activity.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../../web/activity.css"),
                )
            }),
        )
        .route(
            "/activity.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/activity.mjs"),
                )
            }),
        )
        .route(
            "/activity-format.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/activity-format.mjs"),
                )
            }),
        )
        .route(
            "/activity-feed.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/activity-feed.mjs"),
                )
            }),
        )
        .route(
            "/activity-detail.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/activity-detail.mjs"),
                )
            }),
        )
        .route(
            "/activity-sources.mjs",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../../web/activity-sources.mjs"),
                )
            }),
        )
        .route("/api/activity/snapshot", get(snapshot))
        .route("/api/activity/items", get(items))
        .route("/api/activity/bins", get(bins))
        .route("/api/activity/problems", get(problems))
        .route("/api/activity/sources", get(sources))
        .route("/api/activity/item/{id}", get(detail))
        .route("/api/activity/events", get(events))
        .with_state(App {
            shared,
            streams: Arc::new(Semaphore::new(super::STREAMS)),
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_filters_and_epochs_fail_explicitly() {
        for (key, value) in [
            ("category", "other"),
            ("view", "other"),
            ("diagnostics", "1"),
            ("since", "-1"),
            ("outcome", "healthy"),
        ] {
            assert!(filters(&BTreeMap::from([(key.into(), value.into())])).is_err());
        }
        assert!(
            filters(&BTreeMap::from([
                ("since".into(), "20".into()),
                ("until".into(), "10".into())
            ]))
            .is_err()
        );
        assert!(
            watermark(
                &Store::default(),
                &BTreeMap::from([("epoch".into(), "old".into())])
            )
            .unwrap_err()
            .contains("restarted")
        );
    }
}

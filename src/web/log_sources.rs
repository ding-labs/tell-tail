//! Same-origin management API. Mutation requires an exact Origin and custom header.
use super::{Delivery, STREAMS, request_status, secure_response};
use crate::log_sources::{Config, Manager};
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, Request, State},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    convert::Infallible,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;
#[derive(Clone)]
pub(super) struct App {
    pub manager: Arc<Mutex<Manager>>,
    pub host: String,
    pub streams: Arc<Semaphore>,
}
fn error(e: io::Error) -> Response {
    let status = match e.kind() {
        io::ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
        io::ErrorKind::NotFound => StatusCode::NOT_FOUND,
        io::ErrorKind::NotConnected => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    json(status, serde_json::json!({"error":e.to_string()}))
}
fn json(status: StatusCode, value: serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}
impl App {
    async fn apply<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Manager) -> io::Result<T> + Send + 'static,
    ) -> io::Result<T> {
        let manager = self.manager.clone();
        tokio::task::spawn_blocking(move || {
            f(&mut *manager
                .lock()
                .map_err(|_| io::Error::other("Source manager unavailable"))?)
        })
        .await
        .map_err(|_| io::Error::other("Source operation failed"))?
    }
    async fn frame(&self, id: u64) -> io::Result<Delivery> {
        self.apply(move |m| Delivery::encode(m.publication(id)?))
            .await
    }
}
async fn local(State(app): State<App>, request: Request, next: Next) -> Response {
    if request.method() == Method::GET {
        return secure_response(match request_status(&request, &app.host) {
            Some(s) => s.into_response(),
            None => next.run(request).await,
        });
    }
    let headers = request.headers();
    let origin = format!("http://{}", app.host);
    let mut probe = Request::new(Body::empty());
    *probe.uri_mut() = request.uri().clone();
    for key in [header::HOST, header::ORIGIN] {
        for value in headers.get_all(&key) {
            probe.headers_mut().append(key.clone(), value.clone());
        }
    }
    let status = if let Some(status) = request_status(&probe, &app.host) {
        Some(status)
    } else if !matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::DELETE
    ) {
        Some(StatusCode::METHOD_NOT_ALLOWED)
    } else if headers
        .get(header::ORIGIN)
        .is_none_or(|v| v != origin.as_str())
        || headers
            .get("x-tell-tail-action")
            .is_none_or(|v| v != "manage-sources")
        || headers
            .get("sec-fetch-site")
            .is_some_and(|v| v != "same-origin")
    {
        Some(StatusCode::FORBIDDEN)
    } else if headers.contains_key(header::TRANSFER_ENCODING)
        || headers.contains_key(header::UPGRADE)
        || headers
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .is_none_or(|n| n > 8192)
    {
        Some(StatusCode::PAYLOAD_TOO_LARGE)
    } else if request.method() != Method::DELETE
        && headers
            .get(header::CONTENT_TYPE)
            .is_none_or(|v| v != "application/json")
    {
        Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
    } else {
        None
    };
    secure_response(match status {
        Some(s) => s.into_response(),
        None => next.run(request).await,
    })
}
async fn list(State(app): State<App>) -> Response {
    match app.apply(|m| Ok(m.list())).await {
        Ok(v) => json(StatusCode::OK, v),
        Err(e) => error(e),
    }
}
async fn add(State(app): State<App>, body: Bytes) -> Response {
    let config: Config = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid source configuration",
            ));
        }
    };
    match app.apply(move |m| m.add(config)).await {
        Ok(id) => json(StatusCode::CREATED, serde_json::json!({"id":id})),
        Err(e) => error(e),
    }
}
async fn edit(State(app): State<App>, Path(id): Path<u64>, body: Bytes) -> Response {
    let config: Config = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid source configuration",
            ));
        }
    };
    match app.apply(move |m| m.edit(id, config)).await {
        Ok(()) => json(StatusCode::OK, serde_json::json!({"id":id})),
        Err(e) => error(e),
    }
}
async fn action(State(app): State<App>, Path(id): Path<u64>, body: Bytes) -> Response {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Action {
        action: String,
    }
    let action: Action = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid source action",
            ));
        }
    };
    if action.action != "pause" && action.action != "resume" {
        return error(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Use pause or resume",
        ));
    }
    match app.apply(move |m| m.action(id, &action.action)).await {
        Ok(()) => json(StatusCode::OK, serde_json::json!({"id":id})),
        Err(e) => error(e),
    }
}
async fn remove(State(app): State<App>, Path(id): Path<u64>) -> Response {
    match app.apply(move |m| m.action(id, "remove")).await {
        Ok(()) => json(StatusCode::OK, serde_json::json!({"removed":id})),
        Err(e) => error(e),
    }
}
async fn snapshot(State(app): State<App>, Path(id): Path<u64>) -> Response {
    match app.frame(id).await {
        Ok(Delivery::Snapshot { json, .. }) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (
                    header::HeaderName::from_static("x-tell-tail-streams-available"),
                    app.streams.available_permits().to_string().as_str(),
                ),
            ],
            json,
        )
            .into_response(),
        Ok(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Err(e) => error(e),
    }
}
async fn events(State(app): State<App>, Path(id): Path<u64>) -> Response {
    let Ok(permit) = app.streams.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            format!("At most {STREAMS} log streams"),
        )
            .into_response();
    };
    if let Err(e) = app.frame(id).await {
        return error(e);
    }
    let body = futures_util::stream::unfold(
        (app, permit, true, false),
        move |(app, permit, first, done)| async move {
            if done {
                return None;
            }
            if !first {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let (event,done)=match app.frame(id).await {Ok(frame)=>frame.sse_event(),Err(_)=>(Bytes::from_static(b"event: failure\ndata: {\"message\":\"Source removed or unavailable\"}\n\n"),true)};
            Some((Ok::<_, Infallible>(event), (app, permit, false, done)))
        },
    );
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(body),
    )
        .into_response()
}
pub(super) fn router(app: App) -> Router {
    Router::new()
        .route("/api/log-sources", get(list).post(add))
        .route(
            "/api/log-sources/{id}",
            post(action).put(edit).delete(remove),
        )
        .route("/api/log-sources/{id}/snapshot", get(snapshot))
        .route("/api/log-sources/{id}/events", get(events))
        .layer(axum::extract::DefaultBodyLimit::max(8192))
        .layer(middleware::from_fn_with_state(app.clone(), local))
        .with_state(app)
}

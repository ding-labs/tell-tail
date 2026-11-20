//! Fixed compile-time assets; no filesystem reads or external frontend runtime.
use axum::{Router, http::header, response::Html, routing::get};

pub(super) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    files()
        .route(
            "/activity",
            get(|| async { Html(include_str!("../../web/activity-start.html")) }),
        )
        .route(
            "/",
            get(|| async { Html(include_str!("../../web/index.html")) }),
        )
        .route(
            "/logs",
            get(|| async { Html(include_str!("../../web/index.html")) }),
        )
}

pub(super) fn files<S: Clone + Send + Sync + 'static>() -> Router<S> {
    let mut routes = Router::new();
    for (path, mime, content) in [
        (
            "/workspace.css",
            "text/css; charset=utf-8",
            include_str!("../../web/workspace.css"),
        ),
        (
            "/workspace.mjs",
            "text/javascript; charset=utf-8",
            include_str!("../../web/workspace.mjs"),
        ),
        (
            "/preferences.mjs",
            "text/javascript; charset=utf-8",
            include_str!("../../web/preferences.mjs"),
        ),
        (
            "/dashboard.css",
            "text/css; charset=utf-8",
            include_str!("../../web/dashboard.css"),
        ),
        (
            "/dashboard.mjs",
            "text/javascript; charset=utf-8",
            include_str!("../../web/dashboard.mjs"),
        ),
        (
            "/frame.mjs",
            "text/javascript; charset=utf-8",
            include_str!("../../web/frame.mjs"),
        ),
        (
            "/render.mjs",
            "text/javascript; charset=utf-8",
            include_str!("../../web/render.mjs"),
        ),
        (
            "/logo.svg",
            "image/svg+xml",
            include_str!("../../web/logo.svg"),
        ),
        (
            "/favicon.svg",
            "image/svg+xml",
            include_str!("../../web/favicon.svg"),
        ),
    ] {
        routes = routes.route(
            path,
            get(move || async move { ([(header::CONTENT_TYPE, mime)], content) }),
        );
    }
    routes
}

//! Fixed compile-time assets; no filesystem reads or external frontend runtime.
use super::App;
use axum::{Router, http::header, response::Html, routing::get};

pub(super) fn routes() -> Router<App> {
    let mut routes = Router::new().route(
        "/",
        get(|| async { Html(include_str!("../../web/index.html")) }),
    );
    for (path, mime, content) in [
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

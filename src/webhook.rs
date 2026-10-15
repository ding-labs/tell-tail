//! Internal HTTP helper. The supervisor can terminate even a stalled DNS lookup.
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read},
    time::Duration,
};

pub const HELPER_ARG: &str = "--__tell-tail-webhook-worker";
// Covers a maximally escaped message and source path within a fixed IPC bound.
pub const MAX_REQUEST: usize = 128 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub url: String,
    pub body: String,
}

pub fn validate_url(value: &str) -> Result<(), &'static str> {
    let invalid = "--alert-webhook must be an HTTP(S) URL without credentials or a fragment (at most 2048 bytes)";
    let uri: ureq::http::Uri = value.parse().map_err(|_| invalid)?;
    if value.len() > 2048
        || value.contains('#')
        || !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.host().is_none_or(str::is_empty)
        || uri.authority().is_none_or(|a| a.as_str().contains('@'))
    {
        return Err(invalid);
    }
    Ok(())
}

pub fn is_helper() -> bool {
    let mut args = std::env::args_os().skip(1);
    args.next().is_some_and(|arg| arg == HELPER_ARG) && args.next().is_none()
}

/// One bounded length-prefixed request on stdin. The parent keeps the pipe open
/// throughout delivery; EOF cancels the helper if the parent dies. Exit codes:
/// 0 = HTTP 2xx, 10 = other HTTP status, 11 = transport/protocol failure.
pub fn run_helper() -> i32 {
    let request = (|| -> Option<Request> {
        let mut input = io::stdin().lock();
        let mut header = [0; 4];
        input.read_exact(&mut header).ok()?;
        let len = u32::from_le_bytes(header) as usize;
        if len == 0 || len > MAX_REQUEST {
            return None;
        }
        let mut bytes = vec![0; len];
        input.read_exact(&mut bytes).ok()?;
        let request: Request = serde_json::from_slice(&bytes).ok()?;
        validate_url(&request.url).ok()?;
        Some(request)
    })();
    let Some(request) = request else { return 11 };
    if std::thread::Builder::new()
        .name("webhook-control".into())
        .spawn(|| {
            let _ = io::stdin().read(&mut [0]);
            std::process::exit(11);
        })
        .is_err()
    {
        return 11;
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .max_response_header_size(16 * 1024)
        .input_buffer_size(8192)
        .output_buffer_size(8192)
        .max_idle_connections(0)
        .build()
        .into();
    // Never print URLs, response bodies, or transport errors: endpoints often
    // contain credentials in their path/query. No response body is retained.
    match agent
        .post(&request.url)
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            concat!("tell-tail/", env!("CARGO_PKG_VERSION")),
        )
        .send(request.body.as_bytes())
    {
        Ok(response) if response.status().is_success() => 0,
        Ok(_) => 10,
        Err(_) => 11,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_explicit_http_endpoints_without_userinfo() {
        for url in [
            "https://example.com/hook?token=secret",
            "http://127.0.0.1:9999",
            "http://[::1]:8080/hook",
        ] {
            assert!(validate_url(url).is_ok(), "{url}");
        }
        for url in [
            "",
            "example.com",
            "file:///tmp/hook",
            "https:///",
            "http://user:secret@example.com",
            "https://example.com/#fragment",
            "http://example.com/\nsecret",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert!(validate_url(&format!("https://example.com/{}", "a".repeat(2048))).is_err());
    }
}

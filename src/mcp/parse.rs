//! Host adapters produce allowlisted metadata, never raw tool payloads.
use super::{
    model::{Host, Kind, Parsed, Reason, label},
    time::timestamp_ms,
};
use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

pub fn fingerprint(body: &str) -> u64 {
    let mut h = DefaultHasher::new();
    body.hash(&mut h);
    h.finish()
}
fn clipped(s: &str, limit: usize) -> &str {
    let mut n = s.len().min(limit);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}

/// Quoted logfmt values may contain spaces, escaped quotes and escaped newlines.
/// Unknown fields are skipped, rather than deserializing nested diagnostic data.
fn fields(mut text: &str) -> BTreeMap<&str, String> {
    let mut out = BTreeMap::new();
    for _ in 0..64 {
        text = text.trim_start();
        if text.is_empty() {
            break;
        }
        let end = text.find(char::is_whitespace).unwrap_or(text.len());
        let Some(eq) = text[..end].find('=') else {
            text = &text[end..];
            continue;
        };
        let key = &text[..eq];
        text = &text[eq + 1..];
        let value;
        if text.starts_with('"') {
            let mut stream = serde_json::Deserializer::from_str(text).into_iter::<String>();
            let Some(Ok(v)) = stream.next() else {
                break;
            };
            text = &text[stream.byte_offset()..];
            value = v;
        } else {
            let end = text.find(char::is_whitespace).unwrap_or(text.len());
            value = text[..end].to_owned();
            text = &text[end..];
        }
        if matches!(
            key,
            "server" | "status" | "failureReason" | "threadId" | "hostId" | "error"
        ) {
            // Duplicate fields are ambiguous. Preserve neither value.
            if out.contains_key(key) {
                out.insert(key, String::new());
            } else {
                out.insert(key, value);
            }
        }
    }
    out
}

fn reason(failure: &str, error: &str) -> Reason {
    let error = clipped(error, 8192).to_ascii_lowercase();
    if failure == "reauthenticationRequired"
        || error.contains("invalid_grant")
        || error.contains("reauthentication")
    {
        Reason::Reauthentication
    } else if error.contains("auth required")
        || error.contains("not logged in")
        || error.contains("authorization required")
        || error.contains("unauthorized")
        || error.contains("401")
        || error.contains("403")
    {
        Reason::Authentication
    } else if error.contains("timeout") || error.contains("timed out") {
        Reason::Timeout
    } else if error.contains("enoent")
        || error.contains("no such file")
        || error.contains("executable not found")
    {
        Reason::MissingExecutable
    } else if error.contains("transport")
        || error.contains("connection")
        || error.contains("error sending request")
        || error.contains("metadata discovery")
    {
        Reason::Transport
    } else {
        Reason::Other
    }
}

pub fn codex(line: &str) -> Option<Parsed> {
    let (stamp, tail) = line.split_once(' ')?;
    let (level, tail) = tail.split_once(' ')?;
    let level = match level {
        "info" => "info",
        "warning" | "warn" => "warning",
        "error" => "error",
        "debug" => "debug",
        _ => "unknown",
    };
    let (component, body) = tail.split_once("] ")?;
    let startup = component == "[AppServerConnection"
        && body.starts_with("mcp_server_startup_status_updated ");
    let discovery = component == "[electron-message-handler"
        && body.starts_with("Failed to discover app tools ");
    if !startup && !discovery {
        if !body.starts_with("mcp_")
            || !matches!(
                component,
                "[AppServerConnection" | "[electron-message-handler" | "[mcp-app-sandbox"
            )
        {
            return None;
        }
        return Some(Parsed {
            host: Host::CodexDesktop,
            server: "unknown-server".into(),
            scope: None,
            timestamp_ms: timestamp_ms(stamp),
            level,
            kind: Kind::Unknown,
            reason: None,
            method: None,
            fingerprint: fingerprint(body),
            aggregate: false,
        });
    }
    let f = fields(body);
    let get = |key| f.get(key).map(String::as_str).unwrap_or("");
    let kind = if discovery {
        Kind::DiscoveryFailed
    } else {
        match get("status") {
            "starting" => Kind::Starting,
            "ready" => Kind::Ready,
            "failed" => Kind::Failed,
            "cancelled" => Kind::Cancelled,
            _ => Kind::Unknown,
        }
    };
    let scope = (!get("threadId").is_empty()
        && get("threadId") != "null"
        && get("threadId").len() <= 256
        && get("hostId").len() <= 256)
        .then(|| format!("{}:{}", get("hostId"), get("threadId")));
    Some(Parsed {
        host: Host::CodexDesktop,
        server: label(get("server")),
        scope,
        timestamp_ms: timestamp_ms(stamp),
        level,
        kind,
        reason: kind
            .is_failure()
            .then(|| reason(get("failureReason"), get("error"))),
        method: None,
        fingerprint: fingerprint(body),
        aggregate: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codex_fixture_semantics_and_unknown_formats() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/mcp/codex.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let p = codex(case["line"].as_str().unwrap());
            if case["kind"].is_null() {
                assert!(p.is_none());
                continue;
            }
            let p = p.unwrap();
            assert_eq!(serde_json::to_value(p.kind).unwrap(), case["kind"]);
            assert_eq!(serde_json::to_value(p.reason).unwrap(), case["reason"]);
        }
    }
    #[test]
    fn escaped_fields_do_not_become_injected_keys() {
        let line = r#"2026-11-12T00:00:00Z warning [AppServerConnection] mcp_server_startup_status_updated error="auth required\n server=evil status=ready \"secret\"" server=example status=failed threadId=PRIVATE hostId=local"#;
        let p = codex(line).unwrap();
        assert_eq!(p.server, "example");
        assert_eq!(p.kind, Kind::Failed);
        assert_eq!(p.scope.as_deref(), Some("local:PRIVATE"));
        assert_eq!(p.reason, Some(Reason::Authentication));
        assert_eq!(
            codex(&line.replace(
                "status=failed threadId",
                "status=failed status=ready threadId"
            ))
            .unwrap()
            .kind,
            Kind::Unknown
        );
    }
}

pub fn claude(line: &str) -> Option<Parsed> {
    let (stamp, tail) = line.split_once(' ')?;
    let (first, tail) = tail.strip_prefix('[')?.split_once("] ")?;
    let (second, body) = tail.strip_prefix('[')?.split_once("] ")?;
    let aggregate = matches!(first, "info" | "warn" | "warning" | "error" | "debug");
    let (server, raw_level) = if aggregate {
        (second, first)
    } else {
        (first, second)
    };
    let level = match raw_level {
        "info" => "info",
        "warn" | "warning" => "warning",
        "error" => "error",
        "debug" => "debug",
        _ => "unknown",
    };
    let body = body.rsplit_once(" { metadata:").map_or(body, |(b, _)| b);
    let kind = if body.starts_with("Initializing server") {
        Kind::Starting
    } else if body.starts_with("Server started and connected successfully") {
        Kind::Ready
    } else if body.starts_with("Server transport closed (intentional shutdown)")
        || body.starts_with("Shutting down server")
    {
        Kind::IntentionalClose
    } else if body.starts_with("Server transport closed (renderer released port)") {
        Kind::RendererRelease
    } else if body.starts_with("Server transport closed unexpectedly")
        || body.starts_with("Server disconnected.")
    {
        Kind::UnexpectedClose
    } else if body.starts_with("Server transport closed")
        || body.starts_with("Client transport closed")
    {
        Kind::Closed
    } else if body.starts_with("Message from client:") || body.starts_with("Message from server:") {
        Kind::Protocol
    } else if level == "error" {
        Kind::DiagnosticFailed
    } else {
        Kind::Unknown
    };
    // Deserialize only the top-level method. Arguments/results are ignored.
    #[derive(serde::Deserialize)]
    struct Method {
        method: Option<String>,
    }
    let metadata = body.split_once(": ").map(|(_, tail)| tail).unwrap_or("");
    let parsed_method = if kind == Kind::Protocol && metadata.starts_with('{') {
        serde_json::from_str::<Method>(metadata)
            .ok()
            .and_then(|m| m.method)
    } else {
        metadata
            .strip_prefix("method=\"")
            .and_then(|s| s.split_once('"'))
            .map(|(m, _)| m.to_owned())
    };
    let method = [
        "initialize",
        "notifications/initialized",
        "tools/list",
        "resources/list",
        "tools/call",
        "ping",
    ]
    .into_iter()
    .find(|m| Some(*m) == parsed_method.as_deref());
    Some(Parsed {
        host: Host::ClaudeDesktop,
        server: label(server),
        scope: None,
        timestamp_ms: timestamp_ms(stamp),
        level,
        kind,
        reason: if kind == Kind::UnexpectedClose {
            Some(Reason::Transport)
        } else {
            kind.is_failure().then(|| reason("", body))
        },
        method,
        fingerprint: fingerprint(body),
        aggregate,
    })
}

pub fn record(host: Host, bytes: &[u8]) -> Option<Parsed> {
    let line = std::str::from_utf8(bytes).ok()?.trim_end_matches('\r');
    match host {
        Host::CodexDesktop => codex(line),
        Host::ClaudeDesktop => claude(line),
    }
}

#[cfg(test)]
mod claude_tests {
    use super::*;
    #[test]
    fn mirrored_formats_match_without_retaining_private_payloads() {
        let a=claude("2026-11-12T00:00:00.000Z [info] [example] Message from client: {\"method\":\"tools/call\",\"params\":{\"secret\":\"PRIVATE\"}}").unwrap();
        let b=claude("2026-11-12T00:00:00.031Z [example] [info] Message from client: {\"method\":\"tools/call\",\"params\":{\"secret\":\"PRIVATE\"}} { metadata: undefined }").unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert!(a.aggregate);
        assert!(!b.aggregate);
        assert_eq!(a.method, Some("tools/call"));
        assert_eq!(a.kind, Kind::Protocol);
        assert_eq!(b.timestamp_ms.unwrap() - a.timestamp_ms.unwrap(), 31);
    }
    #[test]
    fn lifecycle_semantics_beat_log_severity() {
        for (body, kind) in [
            ("Initializing server...", Kind::Starting),
            ("Server started and connected successfully", Kind::Ready),
            (
                "Server transport closed (intentional shutdown)",
                Kind::IntentionalClose,
            ),
            (
                "Server transport closed (renderer released port)",
                Kind::RendererRelease,
            ),
            (
                "Server transport closed unexpectedly, process exiting early",
                Kind::UnexpectedClose,
            ),
            ("Client transport closed", Kind::Closed),
        ] {
            let p = claude(&format!("2026-11-12T00:00:00Z [example] [info] {body}")).unwrap();
            assert_eq!(p.kind, kind);
        }
        let p =
            claude("2026-11-12T00:00:00Z [error] [example] failed: ENOENT /private/path").unwrap();
        assert_eq!(p.kind, Kind::DiagnosticFailed);
        assert_eq!(p.reason, Some(Reason::MissingExecutable));
        assert!(claude("unframed private stderr content").is_none());
    }
}

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

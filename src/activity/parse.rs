//! Versioned allowlist for observed desktop log shapes. Unknown bodies are discarded.
use super::model::{Category, Evidence, Item};
use crate::mcp::{collection::Record, model::Host};
use std::collections::BTreeMap;

pub enum Parsed {
    Event(Box<Item>),
    Unsupported,
    Ignored,
}

// Scan quoted and nested unknown values without deserializing their contents.
fn value_end(text: &str) -> Option<usize> {
    let first = *text.as_bytes().first()?;
    if first != b'"' && first != b'{' && first != b'[' {
        return Some(text.find(char::is_whitespace).unwrap_or(text.len()));
    }
    let mut quoted = false;
    let mut escape = false;
    let mut depth = 0u8;
    for (i, b) in text.bytes().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if quoted && b == b'\\' {
            escape = true;
        } else if b == b'"' {
            quoted = !quoted;
            if !quoted && depth == 0 {
                return Some(i + 1);
            }
        } else if !quoted {
            if b == b'{' || b == b'[' {
                depth = depth.checked_add(1).filter(|d| *d <= 16)?;
            } else if b == b'}' || b == b']' {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
        }
    }
    None
}

fn fields(mut text: &str) -> Option<BTreeMap<&str, Option<String>>> {
    let mut out = BTreeMap::new();
    for _ in 0..64 {
        text = text.trim_start();
        if text.is_empty() {
            return Some(out);
        }
        let eq = text.find('=')?;
        let key = &text[..eq];
        if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        text = &text[eq + 1..];
        let end = value_end(text)?;
        let value = &text[..end];
        text = &text[end..];
        if !text.is_empty() && !text.starts_with(char::is_whitespace) {
            return None;
        }
        if matches!(key, "method" | "durationMs" | "retryCount") {
            let value = if value.len() > 96 {
                None
            } else if value.starts_with('"') {
                serde_json::from_str::<String>(value).ok()
            } else {
                Some(value.to_owned())
            };
            if out.contains_key(key) {
                out.insert(key, None);
            } else {
                out.insert(key, value);
            }
        }
    }
    None
}

pub fn record(r: &Record<'_>, now: i64) -> Parsed {
    if r.host != Host::CodexDesktop || r.bytes.len() > crate::mcp::model::MAX_RECORD {
        return Parsed::Ignored;
    }
    let Ok(line) = std::str::from_utf8(r.bytes) else {
        return Parsed::Unsupported;
    };
    let Some((stamp, rest)) = line.trim_end().split_once(' ') else {
        return Parsed::Ignored;
    };
    let Some((level, rest)) = rest.split_once(' ') else {
        return Parsed::Ignored;
    };
    let Some((component, body)) = rest.strip_prefix('[').and_then(|s| s.split_once("] ")) else {
        return Parsed::Ignored;
    };
    let (name, body) = body.split_once(' ').unwrap_or((body, ""));
    if name.starts_with("mcp_") {
        return Parsed::Ignored;
    }
    let (category, kind, summary, subject, outcome, diagnostic) = match (component, name) {
        ("electron-message-handler", "chatgpt_pubsub_transport_opened") => (
            Category::Connections,
            "transport_opened",
            "ChatGPT transport opened",
            "ChatGPT connection",
            "opened_observed",
            false,
        ),
        ("electron-message-handler", "chatgpt_pubsub_transport_closed") => (
            Category::Connections,
            "transport_closed",
            "ChatGPT transport closed",
            "ChatGPT connection",
            "closed_observed",
            false,
        ),
        ("electron-message-handler", "chatgpt_pubsub_reconnect_scheduled") => (
            Category::Connections,
            "reconnect_scheduled",
            "ChatGPT reconnect scheduled",
            "ChatGPT connection",
            "retry_scheduled",
            false,
        ),
        ("electron-message-handler", "chatgpt_pubsub_topic_subscribed") => (
            Category::Connections,
            "topic_subscribed",
            "ChatGPT subscription established",
            "ChatGPT connection",
            "observed",
            true,
        ),
        ("AppServerConnection" | "electron-message-handler", "plugin_install_succeeded") => (
            Category::Desktop,
            "plugin_installed",
            "Plugin installation completed",
            "Plugin lifecycle",
            "completed",
            false,
        ),
        ("AppServerConnection", "plugin_marketplace_add_succeeded") => (
            Category::Desktop,
            "marketplace_added",
            "Plugin marketplace added",
            "Plugin lifecycle",
            "completed",
            false,
        ),
        ("BundledPluginsMarketplace", "plugin_marketplace_sync_completed") => (
            Category::Desktop,
            "marketplace_synced",
            "Plugin marketplace sync completed",
            "Plugin lifecycle",
            "completed",
            false,
        ),
        ("browser-session-registry", "browser_use_iab_backend_startup_started") => (
            Category::Desktop,
            "browser_starting",
            "Browser runtime startup began",
            "Browser runtime",
            "start_observed",
            false,
        ),
        ("browser-session-registry", "browser_use_iab_backend_startup_ready") => (
            Category::Desktop,
            "browser_ready",
            "Browser runtime reported ready",
            "Browser runtime",
            "ready_observed",
            false,
        ),
        ("install-primary-runtime", "primary_runtime_dependencies_diagnose_started") => (
            Category::Desktop,
            "runtime_check_started",
            "Runtime dependency check began",
            "Local runtime",
            "start_observed",
            false,
        ),
        ("install-primary-runtime", "primary_runtime_dependencies_diagnose_finished") => (
            Category::Desktop,
            "runtime_check_finished",
            "Runtime dependency check finished",
            "Local runtime",
            "completed",
            false,
        ),
        ("AppServerConnection", "response_routed") => (
            Category::Desktop,
            "response_routed",
            "App-server response routed",
            "App server",
            "observed",
            true,
        ),
        _ if matches!(
            component,
            "electron-message-handler"
                | "AppServerConnection"
                | "BundledPluginsMarketplace"
                | "browser-session-registry"
                | "install-primary-runtime"
        ) =>
        {
            return Parsed::Unsupported;
        }
        _ => return Parsed::Ignored,
    };
    let Some(fields) = fields(body) else {
        return Parsed::Unsupported;
    };
    let get = |key| fields.get(key).and_then(Option::as_deref);
    let mut item = Item::observation(category, kind, summary);
    item.subject = subject.into();
    item.outcome = outcome;
    item.diagnostic = diagnostic;
    item.source_level = match level {
        "info" => "info",
        "warn" | "warning" => "warning",
        "error" => "error",
        "debug" => "debug",
        _ => "unknown",
    };
    item.occurred_ms = crate::mcp::time::timestamp_ms(stamp);
    item.received_ms = now;
    item.historical = r.historical;
    item.source_ref = format!("file-{}", r.source_id);
    item.evidence.push(Evidence {
        source: item.source_ref.clone(),
        generation: r.generation,
        offset: r.offset,
    });
    if kind == "reconnect_scheduled" {
        item.attempt = get("retryCount")
            .and_then(|v| v.parse().ok())
            .filter(|n| *n <= 1_000_000);
        item.note = "A retry was scheduled. Its success and effects on chats are not established by this event.";
    } else if kind == "transport_closed" {
        item.note = "A closed transport is not proof of failure or an interrupted chat. Source log level is preserved separately.";
    } else if kind == "response_routed" {
        item.scope = "unknown";
        item.method = match get("method") {
            Some("thread/read") => Some("thread/read"),
            Some("thread/list") => Some("thread/list"),
            Some("thread/start") => Some("thread/start"),
            Some("thread/resume") => Some("thread/resume"),
            Some("turn/start") => Some("turn/start"),
            Some("turn/interrupt") => Some("turn/interrupt"),
            Some("config/read") => Some("config/read"),
            Some("account/read") => Some("account/read"),
            Some("skills/list") => Some("skills/list"),
            Some("plugin/list") => Some("plugin/list"),
            Some("model/list") => Some("model/list"),
            Some("app/list") => Some("app/list"),
            Some("mcpServerStatus/list") => Some("mcpServerStatus/list"),
            _ => None,
        };
        item.duration_ms = get("durationMs")
            .and_then(|s| s.parse().ok())
            .filter(|n| *n <= 86_400_000);
        item.duration_basis = item.duration_ms.map(|_| "source_reported");
        item.note = "Routing duration is source-reported; it is not established model or tool latency. Error codes and request IDs are omitted.";
    }
    Parsed::Event(Box::new(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(line: &str) -> Parsed {
        record(
            &Record {
                source_id: 1,
                generation: 2,
                offset: 10,
                historical: true,
                host: Host::CodexDesktop,
                bytes: line.as_bytes(),
            },
            99,
        )
    }
    #[test]
    fn audited_shapes_exclude_sensitive_values_and_preserve_semantics() {
        let items: Vec<_> = include_str!("../../tests/fixtures/desktop-activity.log")
            .lines()
            .filter_map(|l| match parse(l) {
                Parsed::Event(i) => Some(i),
                _ => None,
            })
            .collect();
        assert_eq!(items.len(), 12);
        let encoded = serde_json::to_string(&items).unwrap();
        assert!(!encoded.contains("PRIVATE"));
        assert_eq!(items[1].severity, super::super::model::Severity::Info);
        assert_eq!(items[1].source_level, "warning");
        assert_eq!(items[2].attempt, Some(2));
        assert_eq!(items[11].duration_ms, Some(123));
        assert_eq!(items[11].method, Some("thread/read"));
    }
    #[test]
    fn quoted_nested_duplicate_and_unbounded_fields_do_not_leak() {
        let prefix = "2026-11-19T12:00:00Z info [AppServerConnection] response_routed ";
        let Parsed::Event(i) = parse(&format!(
            "{prefix}body={{\"nested\":\"durationMs=999 PRIVATE\"}} durationMs=4 durationMs=8 method=\"PRIVATE_METHOD\""
        )) else {
            panic!()
        };
        assert_eq!(i.duration_ms, None);
        assert_eq!(i.method, None);
        assert!(!serde_json::to_string(&i).unwrap().contains("PRIVATE"));
        assert!(matches!(
            parse(&format!("{prefix}body=\"unclosed")),
            Parsed::Unsupported
        ));
        assert!(matches!(
            parse("2026-11-19T12:00:00Z info [unknown] response_routed"),
            Parsed::Ignored
        ));
    }
    proptest::proptest! {
        #[test]
        fn arbitrary_input_never_panics(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..8192)) {
            let _ = record(&Record { source_id: 0, generation: 0, offset: 0, historical: false, host: Host::CodexDesktop, bytes: &bytes }, 0);
        }
    }
}

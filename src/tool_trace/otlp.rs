//! Restricted OTLP logs/protobuf adapter for the verified Codex tool_result shape.
//! Payload fields and resource attributes are skipped without allocating them.
use super::model::{Observation, Phase, SCHEMA_VERSION};
use std::collections::BTreeMap;

pub const MAX_BODY: usize = 1024 * 1024;
const MAX_RECORDS: usize = 1024;
const MAX_FIELDS: usize = 32_768;
#[derive(Debug)]
pub struct Invalid(pub &'static str);
#[derive(Clone, Copy)]
enum Value<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Fixed(u64),
    Other,
}
struct Wire<'a> {
    input: &'a [u8],
    budget: usize,
}
impl<'a> Wire<'a> {
    fn varint(&mut self) -> Result<u64, Invalid> {
        let mut n = 0;
        for shift in (0..70).step_by(7) {
            let (&b, rest) = self
                .input
                .split_first()
                .ok_or(Invalid("malformed_protobuf"))?;
            self.input = rest;
            if shift == 63 && b > 1 {
                return Err(Invalid("malformed_protobuf"));
            }
            n |= u64::from(b & 127) << shift;
            if b < 128 {
                return Ok(n);
            }
        }
        Err(Invalid("malformed_protobuf"))
    }
    fn next(&mut self) -> Result<Option<(u32, Value<'a>)>, Invalid> {
        if self.input.is_empty() {
            return Ok(None);
        }
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or(Invalid("malformed_protobuf"))?;
        let tag = self.varint()?;
        if tag >> 3 == 0 || tag >> 3 >= 1 << 29 {
            return Err(Invalid("malformed_protobuf"));
        }
        let wire = tag & 7;
        let v = match wire {
            0 => Value::Int(self.varint()?),
            1 | 2 | 5 => {
                let len = if wire == 2 {
                    usize::try_from(self.varint()?).map_err(|_| Invalid("malformed_protobuf"))?
                } else if wire == 1 {
                    8
                } else {
                    4
                };
                let (bytes, rest) = self
                    .input
                    .split_at_checked(len)
                    .ok_or(Invalid("malformed_protobuf"))?;
                self.input = rest;
                if wire == 1 {
                    Value::Fixed(u64::from_le_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| Invalid("malformed_protobuf"))?,
                    ))
                } else if wire == 2 {
                    Value::Bytes(bytes)
                } else {
                    Value::Other
                }
            }
            _ => return Err(Invalid("malformed_protobuf")),
        };
        Ok(Some(((tag >> 3) as u32, v)))
    }
}
fn walk<'a>(
    input: &'a [u8],
    budget: &mut usize,
    mut f: impl FnMut(u32, Value<'a>, &mut usize) -> Result<(), Invalid>,
) -> Result<(), Invalid> {
    let mut w = Wire {
        input,
        budget: *budget,
    };
    while let Some((n, v)) = w.next()? {
        f(n, v, &mut w.budget)?;
    }
    *budget = w.budget;
    Ok(())
}
#[derive(Clone, Copy)]
enum Scalar<'a> {
    Text(&'a str),
    Int(u64),
    Bool(bool),
}
fn scalar<'a>(
    b: &'a [u8],
    budget: &mut usize,
    max_text: usize,
) -> Result<Option<Scalar<'a>>, Invalid> {
    let mut result = None;
    let mut count = 0;
    walk(b, budget, |n, v, _| {
        count += 1;
        if count > 1 {
            return Err(Invalid("malformed_protobuf"));
        }
        result = match (n, v) {
            (1, Value::Bytes(s)) if s.len() <= max_text => {
                std::str::from_utf8(s).ok().map(Scalar::Text)
            }
            (2, Value::Int(v)) if v <= 1 => Some(Scalar::Bool(v == 1)),
            (3, Value::Int(v)) => Some(Scalar::Int(v)),
            _ => None,
        };
        Ok(())
    })?;
    Ok(result)
}
const ATTRS: &[&str] = &[
    "event.name",
    "event.timestamp",
    "tool_name",
    "tool_namespace",
    "mcp_server",
    "call_id",
    "conversation.id",
    "duration_ms",
    "success",
];
fn attribute<'a>(
    b: &'a [u8],
    budget: &mut usize,
    inspect: bool,
) -> Result<Option<(&'a str, Scalar<'a>)>, Invalid> {
    let mut key = None;
    let mut value = None;
    let mut seen = 0u8;
    walk(b, budget, |n, v, _| {
        if n == 1 || n == 2 {
            let bit = 1 << n;
            if seen & bit != 0 {
                return Err(Invalid("malformed_protobuf"));
            }
            seen |= bit;
        }
        match (n, v) {
            (1, Value::Bytes(s)) if s.len() <= 64 => key = std::str::from_utf8(s).ok(),
            (2, Value::Bytes(s)) => value = Some(s),
            _ => (),
        };
        Ok(())
    })?;
    let Some(key) =
        key.filter(|k| ATTRS.contains(k) || (inspect && matches!(*k, "arguments" | "output")))
    else {
        return Ok(None);
    };
    Ok(match value {
        Some(v) => scalar(
            v,
            budget,
            if matches!(key, "output" | "arguments") {
                MAX_BODY
            } else {
                256
            },
        )?
        .map(|v| (key, v)),
        None => None,
    })
}
fn text<'a>(v: Option<&Scalar<'a>>) -> Option<&'a str> {
    match v {
        Some(Scalar::Text(v)) => Some(v),
        _ => None,
    }
}
fn number(v: Option<&Scalar<'_>>) -> Option<u64> {
    match v {
        Some(Scalar::Int(n)) => Some(*n),
        Some(Scalar::Text(s))
            if !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()) =>
        {
            s.parse().ok()
        }
        _ => None,
    }
}
fn label(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./:-".contains(&b))
}
fn record(
    b: &[u8],
    budget: &mut usize,
    epoch: &str,
    inspect: bool,
) -> Result<Option<Observation>, Invalid> {
    let mut attrs = BTreeMap::new();
    let mut event = None;
    let mut timestamp = None;
    walk(b, budget, |n, v, budget| {
        match (n, v) {
            (1, Value::Fixed(ns)) if ns > 0 => timestamp = Some((ns / 1_000_000) as i64),
            (6, Value::Bytes(b)) => {
                if let Some((k, v)) = attribute(b, budget, inspect)? {
                    if attrs.insert(k, v).is_some() {
                        return Err(Invalid("malformed_protobuf"));
                    }
                }
            }
            (12, Value::Bytes(b)) if b.len() <= 64 => event = std::str::from_utf8(b).ok(),
            _ => (),
        };
        Ok(())
    })?;
    let named = text(attrs.get("event.name"));
    // Codex's event.name attribute carries the semantic event type. OTLP's
    // separate event_name may identify the tracing callsite instead.
    if named.or(event) != Some("codex.tool_result") {
        return Ok(None);
    }
    let name = text(attrs.get("tool_name"))
        .filter(|s| label(s))
        .ok_or(Invalid("malformed_protobuf"))?;
    let namespace = text(attrs.get("tool_namespace")).filter(|s| label(s));
    // This is a source namespace label, not a manifest ID or display-name guess.
    // Preserve the entire suffix (including underscores) and accept new providers.
    let plugin = namespace
        .and_then(|ns| ns.strip_prefix("mcp__codex_apps__"))
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let tool = namespace.map_or_else(|| name.to_owned(), |ns| format!("{ns}.{name}"));
    let execution = text(attrs.get("conversation.id"));
    let outcome = match attrs.get("success") {
        Some(Scalar::Bool(v)) => Some(*v),
        Some(Scalar::Text("true")) => Some(true),
        Some(Scalar::Text("false")) => Some(false),
        _ => None,
    };
    let o = Observation {
        inspection: inspect.then(|| {
            let mut context = super::inspection::Inspection::capture(
                &tool,
                text(attrs.get("arguments")),
                text(attrs.get("output")),
            );
            context.locator = execution
                .zip(text(attrs.get("call_id")))
                .and_then(|(chat, call)| super::turn::Locator::new(chat, call));
            std::sync::Arc::new(context)
        }),
        schema_version: SCHEMA_VERSION,
        source: "codex-otlp-tool-result-v1".into(),
        source_epoch: epoch.into(),
        execution_id: execution.unwrap_or("unscoped").into(),
        call_id: execution
            .and_then(|_| text(attrs.get("call_id")))
            .map(str::to_owned),
        event_id: None,
        tool,
        server: text(attrs.get("mcp_server"))
            .filter(|s| label(s))
            .map(str::to_owned),
        plugin,
        phase: Phase::Completed,
        timestamp_ms: timestamp.or_else(|| {
            text(attrs.get("event.timestamp")).and_then(crate::mcp::time::timestamp_ms)
        }),
        duration_ms: number(attrs.get("duration_ms")).filter(|n| *n <= 86_400_000),
        execution_success: outcome,
        tool_error: None,
    };
    o.validate().map_err(|_| Invalid("invalid_observation"))?;
    Ok(Some(o))
}
pub struct Batch {
    pub observations: Vec<Observation>,
    pub ignored: usize,
}
pub fn decode(body: &[u8], epoch: &str) -> Result<Batch, Invalid> {
    decode_inspected(body, epoch, false)
}
pub(super) fn decode_inspected(body: &[u8], epoch: &str, inspect: bool) -> Result<Batch, Invalid> {
    if body.len() > MAX_BODY {
        return Err(Invalid("malformed_protobuf"));
    }
    let mut result = Batch {
        observations: Vec::new(),
        ignored: 0,
    };
    let mut records = 0;
    let mut budget = MAX_FIELDS;
    walk(body, &mut budget, |n, v, budget| {
        if let (1, Value::Bytes(resource)) = (n, v) {
            walk(resource, budget, |n, v, budget| {
                if let (2, Value::Bytes(scope)) = (n, v) {
                    walk(scope, budget, |n, v, budget| {
                        if let (2, Value::Bytes(raw)) = (n, v) {
                            records += 1;
                            if records > MAX_RECORDS {
                                return Err(Invalid("malformed_protobuf"));
                            }
                            match record(raw, budget, epoch, inspect)? {
                                Some(o) => result.observations.push(o),
                                None => result.ignored += 1,
                            }
                        }
                        Ok(())
                    })?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vi(mut n: u64) -> Vec<u8> {
        let mut b = Vec::new();
        while n > 127 {
            b.push(n as u8 | 128);
            n >>= 7;
        }
        b.push(n as u8);
        b
    }
    fn msg(n: u64, v: &[u8]) -> Vec<u8> {
        [vi(n * 8 + 2), vi(v.len() as u64), v.to_vec()].concat()
    }
    fn attr(k: &str, v: &str) -> Vec<u8> {
        msg(
            6,
            &[msg(1, k.as_bytes()), msg(2, &msg(1, v.as_bytes()))].concat(),
        )
    }
    fn batch(record: &[u8]) -> Vec<u8> {
        msg(1, &msg(2, &msg(2, record)))
    }
    fn fixture() -> Vec<u8> {
        let mut b = Vec::new();
        for (k, v) in [
            ("event.name", "codex.tool_result"),
            ("tool_name", "_search_docs"),
            ("tool_namespace", "mcp__codex_apps__railway"),
            ("mcp_server", "codex_apps"),
            ("conversation.id", "PRIVATE_CHAT"),
            ("call_id", "PRIVATE_CALL"),
            ("duration_ms", "123"),
            ("success", "true"),
            ("output", "PRIVATE_OUTPUT"),
            ("arguments", "PRIVATE_ARGS"),
        ] {
            b.extend(attr(k, v));
        }
        b
    }
    #[test]
    fn verified_namespaced_result_is_payload_free_and_deduplicated() {
        let b = batch(&fixture());
        let mut store = super::super::store::Store::default();
        for _ in 0..2 {
            let mut d = decode(&b, "epoch").unwrap();
            assert_eq!(d.observations.len(), 1);
            let o = d.observations.pop().unwrap();
            assert_eq!(o.plugin.as_deref(), Some("railway"));
            assert_eq!(o.duration_ms, Some(123));
            store.accept(o, 10, false).unwrap();
        }
        let page = store.page(&Default::default(), None, None).unwrap();
        assert_eq!(page.calls.len(), 1);
        assert!(!page.calls[0].start_observed);
        assert_eq!(page.calls[0].execution_success, Some(true));
        assert!(!serde_json::to_string(&page).unwrap().contains("PRIVATE"));
    }
    #[test]
    fn only_opted_in_shell_input_is_exposed_to_the_private_inspector() {
        let raw = batch(&[
            attr("event.name", "codex.tool_result"), attr("tool_namespace", "functions"),
            attr("tool_name", "exec_command"), attr("arguments", r#"{"cmd":"git diff PRIVATE_ARG","other":"PRIVATE_OTHER"}"#),
            attr("output", "Chunk ID: abc\nWall time: 0.2 seconds\nProcess exited with code 2\nOutput:\nPRIVATE_OUT"),
        ].concat());
        assert!(
            decode(&raw, "epoch").unwrap().observations[0]
                .inspection
                .is_none()
        );
        let mut batch = decode_inspected(&raw, "epoch", true).unwrap();
        let o = batch.observations.pop().unwrap();
        let c = o.inspection.as_deref().unwrap();
        assert_eq!(c.input.as_deref(), Some("git diff PRIVATE_ARG"));
        assert_eq!(c.exit_code, Some(2));
        let call = super::super::store::Store::default()
            .accept(o, 1, false)
            .unwrap()
            .unwrap();
        assert!(!serde_json::to_string(&call).unwrap().contains("PRIVATE"));
    }
    #[test]
    fn event_clock_fills_absent_otlp_time_without_using_receipt_time() {
        let raw = [
            fixture(),
            attr("event.timestamp", "2026-11-21T17:38:06.123Z"),
        ]
        .concat();
        let read = |bytes: &[u8]| {
            decode(&batch(bytes), "epoch")
                .unwrap()
                .observations
                .remove(0)
                .timestamp_ms
        };
        assert_eq!(read(&raw), Some(1795282686123));
        let mut timed = raw;
        timed.push(9);
        timed.extend(1_000_000u64.to_le_bytes());
        assert_eq!(read(&timed), Some(1));
        assert_eq!(read(&fixture()), None);
    }
    #[test]
    fn semantic_event_attribute_takes_precedence_over_otlp_callsite_name() {
        let b = batch(&[fixture(), msg(12, b"event at a diagnostic callsite")].concat());
        assert_eq!(decode(&b, "epoch").unwrap().observations.len(), 1);
    }
    #[test]
    fn unknown_plugin_is_not_guessed() {
        let b = [
            attr("event.name", "codex.tool_result"),
            attr("tool_name", "lookup"),
            attr("tool_namespace", "mcp__unknown"),
        ]
        .concat();
        let d = decode(&batch(&b), "epoch").unwrap();
        assert!(d.observations[0].plugin.is_none());
        assert!(d.observations[0].call_id.is_none());
    }
    #[test]
    fn every_hosted_provider_is_discovered_without_a_catalog_or_allowlist() {
        for provider in ["railway", "google_drive", "ding___test", "new_provider_42"] {
            let raw = [
                attr("event.name", "codex.tool_result"),
                attr("tool_name", "lookup"),
                attr("tool_namespace", &format!("mcp__codex_apps__{provider}")),
            ]
            .concat();
            let result = decode(&batch(&raw), "epoch").unwrap();
            assert_eq!(result.observations[0].plugin.as_deref(), Some(provider));
        }
        let raw = [
            attr("event.name", "codex.tool_result"),
            attr("tool_name", "lookup"),
            attr("tool_namespace", "mcp__codex_apps__"),
        ]
        .concat();
        assert!(
            decode(&batch(&raw), "epoch").unwrap().observations[0]
                .plugin
                .is_none()
        );
    }
    #[test]
    fn malformed_duplicate_and_budgeted_input_rejected() {
        for b in [
            vec![0],
            vec![10, 255],
            vec![255; 11],
            vec![0; MAX_BODY + 1],
            batch(&[fixture(), attr("call_id", "different")].concat()),
        ] {
            assert!(decode(&b, "epoch").is_err());
        }
        let scope = msg(2, &[]).repeat(MAX_RECORDS + 1);
        assert!(decode(&msg(1, &msg(2, &scope)), "epoch").is_err());
        let unknown = msg(4, &[]).repeat(MAX_FIELDS + 1);
        assert!(decode(&unknown, "epoch").is_err());
    }
    #[test]
    fn arbitrary_bytes_never_panic() {
        use proptest::prelude::*;
        proptest!(|(bytes in proptest::collection::vec(any::<u8>(),0..8192))|{let _=decode(&bytes,"epoch");});
    }
}

//! Project two native code-mode relationships. Discard every other span, event and attribute.
use super::*;
use crate::tool_trace::turn::{links::Link, tag};

const KEYS: &[&str] = &[
    "conversation.id",
    "turn_id",
    "call_id",
    "cell.id",
    "tool_name",
    "tool_namespace",
    "tool_source",
];
const OWNER: &[u8] = b"code_mode.handler.execute";
const CHILD: &[u8] = b"codex.code_mode.nested_tool_dispatched";

fn extract(input: &[u8], budget: &mut usize, owner: bool) -> Result<Option<Link>, Invalid> {
    let mut attrs = BTreeMap::new();
    walk(input, budget, |n, v, budget| {
        if n == if owner { 9 } else { 3 }
            && let Value::Bytes(b) = v
            && let Some((k, v)) = attribute(b, budget, KEYS, false)?
            && attrs.insert(k, v).is_some()
        {
            return Err(Invalid("duplicate_link_field"));
        }
        Ok(())
    })?;
    let get = |key| text(attrs.get(key)).filter(|s| !s.is_empty());
    Ok((|| {
        let child_tool = if owner {
            None
        } else {
            if get("tool_source")? != "code_mode" {
                return None;
            }
            let ns = get("tool_namespace")?;
            let name = get("tool_name")?;
            if !label(ns) || !label(name) {
                return None;
            }
            Some(format!("{ns}.{name}"))
        };
        Some(Link {
            chat: tag(get("conversation.id")?),
            turn: tag(get("turn_id")?),
            call: tag(get("call_id")?),
            cell: tag(get("cell.id")?),
            child_tool,
        })
    })())
}

fn shape(
    input: &[u8],
    budget: &mut usize,
    owner: bool,
    links: &mut Vec<Link>,
) -> Result<(), Invalid> {
    let mut name = None;
    walk(input, budget, |n, v, budget| {
        if let Value::Bytes(b) = v {
            if n == if owner { 5 } else { 2 } && name.replace(b).is_some() {
                return Err(Invalid("duplicate_span_name"));
            }
            if owner && n == 11 {
                shape(b, budget, false, links)?
            }
        }
        Ok(())
    })?;
    if name == Some(if owner { OWNER } else { CHILD })
        && let Some(link) = extract(input, budget, owner)?
    {
        if links.len() >= MAX_RECORDS {
            return Err(Invalid("link_limit"));
        }
        links.push(link);
    }
    Ok(())
}

pub(in crate::tool_trace) fn decode_links(body: &[u8]) -> Result<Batch, Invalid> {
    if body.len() > MAX_BODY {
        return Err(Invalid("body_limit"));
    }
    let mut batch = Batch::default();
    let mut budget = 128 * 1024;
    walk(body, &mut budget, |n, v, b| {
        if let (1, Value::Bytes(resource)) = (n, v) {
            walk(resource, b, |n, v, b| {
                if let (2, Value::Bytes(scope)) = (n, v) {
                    walk(scope, b, |n, v, b| {
                        if let (2, Value::Bytes(span)) = (n, v) {
                            shape(span, b, true, &mut batch.links)?;
                        }
                        Ok(())
                    })?
                }
                Ok(())
            })?
        }
        Ok(())
    })?;
    Ok(batch)
}

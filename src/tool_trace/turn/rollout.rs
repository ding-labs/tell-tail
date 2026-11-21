use super::{Evidence, Locator, MAX_TOOLS, Tool, bounded, links::Scope, tag};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const WINDOW: usize = 16 * 1024 * 1024;
const MAX_LINE: usize = 1024 * 1024;

fn find_file(root: &Path, locator: &Locator) -> Option<PathBuf> {
    let mut dirs = Vec::new();
    for day in [
        Some(locator.day),
        locator.day.previous_day(),
        locator.day.next_day(),
    ]
    .into_iter()
    .flatten()
    {
        let (y, m, d) = day.to_calendar_date();
        dirs.push(
            root.join("sessions")
                .join(format!("{y:04}/{:02}/{d:02}", u8::from(m))),
        );
    }
    dirs.push(root.join("archived_sessions"));
    for dir in dirs {
        let Ok(real) = dir.canonicalize() else {
            continue;
        };
        if !real.starts_with(root) {
            continue;
        }
        let Ok(entries) = fs::read_dir(real) else {
            continue;
        };
        for entry in entries.take(8192).flatten() {
            let name = entry.file_name();
            let Some(stem) = name
                .to_str()
                .and_then(|s| s.strip_suffix(".jsonl"))
                .filter(|s| s.starts_with("rollout-"))
            else {
                continue;
            };
            if let Some(id) = stem.get(stem.len().saturating_sub(36)..)
                && tag(id) == locator.chat
                && entry.file_type().ok()?.is_file()
            {
                return Some(entry.path());
            }
        }
    }
    None
}

fn event_turn<'a>(p: &'a Value, chat: &str) -> Option<&'a str> {
    (tag(p.get("thread_id")?.as_str()?) == chat).then_some(p.get("turn_id")?.as_str()?)
}

fn response_turn(p: &Value) -> Option<&str> {
    p.pointer("/internal_chat_message_metadata_passthrough/turn_id")?
        .as_str()
}

// An excerpt is read only while inspecting a failed call. It is never copied into the feed.
fn excerpt(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut offset = text.len().saturating_sub(320);
    while !text.is_char_boundary(offset) {
        offset += 1
    }
    Some(if offset == 0 {
        text.into()
    } else {
        format!("…{}", &text[offset..])
    })
}

fn native_tool(item: &Value) -> Option<Tool> {
    let kind = item.get("type")?.as_str()?;
    let (name, op) = match kind {
        "CommandExecution" => (
            match item["source"].as_str() {
                Some("user_shell") => return None,
                Some("unified_exec_interaction") => "functions.write_stdin",
                Some("unified_exec_startup") => "functions.exec_command",
                _ => "Shell command",
            }
            .into(),
            item.get("command")
                .and_then(|v| v.as_str().or_else(|| v.as_array()?.last()?.as_str()))
                .map(|s| bounded(s, 200)),
        ),
        "WebSearch" => ("web.run".into(), None),
        "McpToolCall" | "MCPToolCall" => {
            let name = item.get("tool")?.as_str()?;
            let server = item.get("server")?.as_str()?;
            (bounded(&format!("{server}.{name}"), 144), None)
        }
        "DynamicToolCall" | "CollabAgentToolCall" => {
            let name = item.get("tool")?.as_str()?;
            let ns = item["namespace"]
                .as_str()
                .or((kind == "CollabAgentToolCall").then_some("collaboration"));
            (
                bounded(
                    &ns.map_or_else(|| name.into(), |ns| format!("{ns}.{name}")),
                    144,
                ),
                None,
            )
        }
        "Extension" => (bounded(item.get("kind")?.as_str()?, 144), None),
        "ImageView" => ("functions.view_image".into(), None),
        "ImageGeneration" => ("Image generation".into(), None),
        "FileChange" | "PatchApply" => ("functions.apply_patch".into(), None),
        _ => return None,
    };
    let code = item
        .get("exit_code")
        .and_then(Value::as_i64)
        .and_then(|v| i32::try_from(v).ok());
    let status = item.get("status").and_then(Value::as_str).unwrap_or("");
    let outcome = if code.is_some_and(|n| n != 0) {
        "nonzero_exit"
    } else if matches!(status, "failed" | "Failed")
        || item.get("success") == Some(&Value::Bool(false))
        || item.get("error").is_some_and(|v| !v.is_null())
        || item.pointer("/result/isError") == Some(&Value::Bool(true))
    {
        "failed"
    } else if matches!(status, "declined" | "cancelled" | "interrupted") {
        "interrupted"
    } else if matches!(status, "completed" | "Completed") || code == Some(0) {
        "completed"
    } else {
        "observed"
    };
    let output_excerpt = if matches!(outcome, "failed" | "nonzero_exit") {
        item.pointer("/error/message")
            .or_else(|| item.get("error").filter(|e| e.is_string()))
            .or_else(|| item.get("aggregated_output"))
            .and_then(Value::as_str)
            .and_then(excerpt)
    } else {
        None
    };
    Some(Tool {
        output_excerpt,
        name,
        operation: op,
        outcome,
        exit_code: code,
        selected: false,
    })
}

pub fn read(
    root: &Path,
    loc: &Locator,
    scope: &Scope,
    live: &HashMap<String, Tool>,
) -> io::Result<Evidence> {
    if scope.conflict {
        return Ok(Evidence::default());
    }
    let root = root.canonicalize()?;
    let Some(path) = find_file(&root, loc) else {
        return Ok(project(&[], loc, scope, live, false));
    };
    let mut file = crate::platform::open_regular(&path)?;
    let len = file.metadata()?.len();
    let mut head = Vec::new();
    (&mut file)
        .take(MAX_LINE as u64 + 1)
        .read_to_end(&mut head)?;
    let first = head.split(|b| *b == b'\n').next().unwrap_or_default();
    let meta: Value = serde_json::from_slice(first).unwrap_or(Value::Null);
    if meta["type"] != "session_meta"
        || meta
            .pointer("/payload/id")
            .and_then(Value::as_str)
            .is_none_or(|s| tag(s) != loc.chat)
    {
        return Ok(Evidence::default());
    }
    let start = len.saturating_sub(WINDOW as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut buffer = Vec::new();
    file.take(len - start).read_to_end(&mut buffer)?;
    Ok(project(&buffer, loc, scope, live, start > 0))
}

fn project(
    data: &[u8],
    loc: &Locator,
    scope: &Scope,
    live: &HashMap<String, Tool>,
    cut: bool,
) -> Evidence {
    let mut evidence = Evidence {
        partial: cut || scope.partial,
        ..Default::default()
    };
    let mut candidates = std::collections::BTreeSet::new();
    if let Some(t) = &scope.turn {
        candidates.insert(t.to_string());
    }
    // Decode headers and recognized tool/event shapes only. Prompt and result bodies never enter the trace store.
    let lines = || {
        data.split(|b| *b == b'\n')
            .skip(usize::from(cut))
            .filter(|line| line.len() <= MAX_LINE && !line.is_empty())
    };
    for line in lines() {
        if memchr::memmem::find(line, b"\"call_id\"").is_none()
            && memchr::memmem::find(line, b"\"item_completed\"").is_none()
        {
            continue;
        }
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let p = &r["payload"];
        let turn = match r["type"].as_str() {
            Some("response_item")
                if matches!(
                    p["type"].as_str(),
                    Some(
                        "function_call"
                            | "custom_tool_call"
                            | "function_call_output"
                            | "custom_tool_call_output"
                    )
                ) && p["call_id"].as_str().is_some_and(|s| tag(s) == loc.call) =>
            {
                response_turn(p)
            }
            Some("event_msg")
                if p["type"] == "item_completed"
                    && p.pointer("/item/id")
                        .and_then(Value::as_str)
                        .is_some_and(|s| tag(s) == loc.call) =>
            {
                event_turn(p, &loc.chat)
            }
            _ => None,
        };
        if let Some(t) = turn {
            candidates.insert(tag(t));
        }
    }
    if candidates.len() != 1 {
        return evidence;
    }
    let turn = candidates.pop_first().unwrap();
    evidence.matched = true;
    let mut tools = Vec::<(String, Tool)>::new();
    let mut failures = HashMap::new();
    for line in lines() {
        let Ok(r) = serde_json::from_slice::<Value>(line) else {
            evidence.partial = true;
            continue;
        };
        let p = &r["payload"];
        let item = &p["item"];
        let (id, tool) = match r["type"].as_str() {
            Some("event_msg")
                if p["type"] == "item_completed"
                    && event_turn(p, &loc.chat).is_some_and(|s| tag(s) == turn) =>
            {
                if item["type"] == "UserMessage" {
                    if evidence.requested_ms.is_none() {
                        evidence.requested_ms = p["started_at_ms"]
                            .as_i64()
                            .or_else(|| p["completed_at_ms"].as_i64());
                    }
                    if let Some(parts) = item["content"].as_array() {
                        for t in parts
                            .iter()
                            .filter(|c| c["type"] == "text")
                            .filter_map(|c| c["text"].as_str())
                        {
                            if evidence.requests.len() < 4 {
                                evidence.requests.push(bounded(t, 2048));
                            } else {
                                evidence.partial = true;
                            }
                        }
                    }
                    continue;
                }
                (item["id"].as_str(), native_tool(item))
            }
            Some("response_item") if response_turn(p).is_some_and(|s| tag(s) == turn) => {
                let id = p["call_id"].as_str();
                if matches!(
                    p["type"].as_str(),
                    Some("function_call_output" | "custom_tool_call_output")
                ) {
                    if let Some(key) = id.map(tag)
                        && live.get(&key).is_some_and(|t| t.failed())
                        && let Some(output) = p["output"].as_str().or_else(|| {
                            p["output"]
                                .as_array()?
                                .iter()
                                .find(|part| {
                                    part["type"] == "text" || part["type"] == "output_text"
                                })?
                                .get("text")?
                                .as_str()
                        })
                        && let Some(text) = excerpt(output)
                    {
                        failures.insert(key, text);
                    }
                    if id.is_some_and(|s| tag(s) == loc.call) {
                        let metadata = &p["internal_chat_message_metadata_passthrough"];
                        if let Some(calls) = metadata["executed_tool_calls"].as_array() {
                            evidence.children = calls
                                .iter()
                                .take(MAX_TOOLS)
                                .filter_map(|c| c["name"].as_str())
                                .map(|name| Tool {
                                    name: bounded(name, 144),
                                    operation: None,
                                    outcome: "observed",
                                    exit_code: None,
                                    selected: false,
                                    output_excerpt: None,
                                })
                                .collect();
                            evidence.children_complete = metadata["tool_calls_complete"]
                                .as_bool()
                                .map(|b| b && calls.len() <= MAX_TOOLS);
                        }
                    }
                    continue;
                }
                if !matches!(
                    p["type"].as_str(),
                    Some("function_call" | "custom_tool_call")
                ) {
                    continue;
                }
                let Some(name) = p["name"].as_str() else {
                    continue;
                };
                let name = match p["namespace"].as_str() {
                    Some(ns) => format!("{ns}.{name}"),
                    None if name == "exec" => "functions.exec".into(),
                    None => name.into(),
                };
                (
                    id,
                    Some(Tool {
                        name: bounded(&name, 144),
                        operation: None,
                        outcome: "observed",
                        exit_code: None,
                        selected: false,
                        output_excerpt: None,
                    }),
                )
            }
            _ => continue,
        };
        let (Some(id), Some(mut tool)) = (id, tool) else {
            continue;
        };
        let key = tag(id);
        if let Some(captured) = live.get(&key) {
            // An explicit process outcome or native failure is stronger than transport success.
            let native_output = tool.output_excerpt.take();
            let native_operation = tool.operation.take();
            let native_outcome = tool.outcome;
            let exit = tool.exit_code;
            tool = captured.clone();
            tool.output_excerpt = native_output;
            tool.operation = tool.operation.or(native_operation);
            tool.exit_code = tool.exit_code.or(exit);
            if matches!(native_outcome, "failed" | "nonzero_exit" | "interrupted")
                && !matches!(tool.outcome, "failed" | "nonzero_exit")
            {
                tool.outcome = native_outcome;
                tool.exit_code = exit;
            }
        }
        tool.selected = key == loc.call;
        if let Some((_, existing)) = tools.iter_mut().find(|(id, _)| id == &key) {
            if !existing.failed() || tool.failed() {
                *existing = tool;
            }
            continue;
        }
        if tools.len() >= MAX_TOOLS {
            evidence.partial = true;
            let victim = tools
                .iter()
                .position(|(_, t)| !t.selected && !t.failed())
                .or_else(|| {
                    tool.selected
                        .then(|| tools.iter().position(|(_, t)| !t.selected))
                        .flatten()
                });
            if let Some(index) = victim {
                tools.remove(index);
            } else {
                continue;
            }
        }
        tools.push((key, tool));
    }
    for (key, tool) in &mut tools {
        if let Some(text) = failures.remove(key) {
            tool.output_excerpt = Some(text)
        }
    }
    if !scope.children.is_empty() {
        let mut native: Vec<_> = evidence.children.iter().map(|t| t.name.as_str()).collect();
        let mut agrees = true;
        for (_, child) in &scope.children {
            if let Some(index) = native.iter().position(|name| *name == child.name) {
                native.remove(index);
            } else {
                agrees = false;
            }
        }
        let complete = evidence.children_complete == Some(true) && agrees && native.is_empty();
        evidence.children_complete = Some(complete);
        evidence.children = scope
            .children
            .iter()
            .map(|(key, child)| {
                tools
                    .iter()
                    .find(|(id, _)| id == key)
                    .map(|(_, t)| t)
                    .unwrap_or(child)
                    .clone()
            })
            .collect();
    }
    for (key, tool) in &scope.tools {
        if tools.iter().any(|(id, _)| id == key) {
            continue;
        }
        if tools.len() == MAX_TOOLS {
            evidence.partial = true;
            if tool.selected || tool.failed() {
                if let Some(idx) = tools.iter().position(|(_, t)| !t.selected && !t.failed()) {
                    tools.remove(idx);
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }
        tools.push((key.clone(), tool.clone()));
    }
    evidence.tools = tools.into_iter().map(|(_, t)| t).collect();
    evidence.partial |= data
        .split(|b| *b == b'\n')
        .any(|line| line.len() > MAX_LINE);
    evidence
}

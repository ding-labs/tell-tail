use super::{Evidence, Locator, Tool, bounded, tag};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const WINDOW: usize = 16 * 1024 * 1024;
const MAX_LINE: usize = 1024 * 1024;
const MAX_TOOLS: usize = 128;

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

fn native_tool(item: &Value) -> Option<Tool> {
    let kind = item.get("type")?.as_str()?;
    let (name, op) = match kind {
        "CommandExecution" => (
            "functions.exec_command".into(),
            item.get("command")
                .and_then(|v| v.as_array())
                .and_then(|v| v.last())
                .and_then(Value::as_str)
                .map(super::super::inspection::shell_operation),
        ),
        "WebSearch" => ("web.run".into(), None),
        "McpToolCall" | "MCPToolCall" => {
            let name = item.get("tool")?.as_str()?;
            let server = item.get("server")?.as_str()?;
            (bounded(&format!("{server}.{name}"), 144), None)
        }
        "DynamicToolCall" => (bounded(item.get("tool")?.as_str()?, 144), None),
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
    Some(Tool {
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
    known_turn: Option<&str>,
    live: &HashMap<String, Tool>,
) -> io::Result<Evidence> {
    let root = root.canonicalize()?;
    let Some(path) = find_file(&root, loc) else {
        return Ok(Evidence::default());
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
        || !meta
            .pointer("/payload/id")
            .and_then(Value::as_str)
            .is_some_and(|s| tag(s) == loc.chat)
    {
        return Ok(Evidence::default());
    }
    let start = len.saturating_sub(WINDOW as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut buffer = Vec::new();
    file.take(len - start).read_to_end(&mut buffer)?;
    Ok(project(&buffer, loc, known_turn, live, start > 0))
}

fn project(
    data: &[u8],
    loc: &Locator,
    known_turn: Option<&str>,
    live: &HashMap<String, Tool>,
    cut: bool,
) -> Evidence {
    let mut evidence = Evidence {
        partial: cut,
        ..Default::default()
    };
    let mut candidates = std::collections::BTreeSet::new();
    if let Some(t) = known_turn {
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
    let mut tools = BTreeMap::<String, usize>::new();
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
            let native_outcome = tool.outcome;
            let exit = tool.exit_code;
            tool = captured.clone();
            if matches!(native_outcome, "failed" | "nonzero_exit" | "interrupted")
                && !matches!(tool.outcome, "failed" | "nonzero_exit")
            {
                tool.outcome = native_outcome;
                tool.exit_code = exit;
            }
        }
        tool.selected = key == loc.call;
        if let Some(idx) = tools.get(&key) {
            evidence.tools[*idx] = tool;
        } else if evidence.tools.len() < MAX_TOOLS {
            tools.insert(key, evidence.tools.len());
            evidence.tools.push(tool);
        } else {
            evidence.partial = true;
        }
    }
    evidence.partial |= data
        .split(|b| *b == b'\n')
        .any(|line| line.len() > MAX_LINE);
    evidence
}

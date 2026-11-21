use super::*;
use serde_json::{Value, json};
use std::{collections::HashMap, fs};

const CHAT: &str = "01900000-0000-7000-b000-000000000001";

fn event(chat: &str, turn: &str, item: Value) -> Value {
    json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":chat,"turn_id":turn,"item":item}})
}
fn call(id: &str, turn: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":id,"internal_chat_message_metadata_passthrough":{"turn_id":turn}}})
}
fn prompt(text: &str) -> Value {
    json!({"type":"UserMessage","content":[{"type":"text","text":text},{"type":"image","url":"private"}]})
}
#[test]
fn exact_chat_call_and_turn_keep_requests_failures_and_sibling_calls_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("archived_sessions");
    fs::create_dir(&folder).unwrap();
    let file = folder.join(format!("rollout-session-{CHAT}.jsonl"));
    let log = [
        json!({"type":"session_meta","payload":{"id":CHAT}}),
        event(CHAT,"older",prompt("wrong turn")),
        event("other","wanted",prompt("wrong chat")),
        event(CHAT,"wanted",prompt("fix these tests")),
        call("wrapper","wanted"),
        call("sibling","wanted"),
        event(CHAT,"wanted",json!({"type":"CommandExecution","id":"child","command":["sh","-c","cargo test"],"status":"completed","exit_code":101})),
        json!({"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"sibling","internal_chat_message_metadata_passthrough":{"turn_id":"wanted","executed_tool_calls":[{"name":"wrong"}]}}}),
        json!({"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"wrapper","internal_chat_message_metadata_passthrough":{"turn_id":"wanted","executed_tool_calls":[{"name":"functions.exec_command","arguments":"private"}],"tool_calls_complete":true}}})
    ].map(|r|r.to_string()).join("\n");
    fs::write(&file, &log).unwrap();
    let loc = Locator::new(CHAT, "wrapper").unwrap();
    let found = read(dir.path(), &loc, &Default::default(), &HashMap::new()).unwrap();
    assert!(found.matched);
    assert_eq!(found.requests, ["fix these tests"]);
    assert_eq!(found.tools.len(), 3);
    assert!(found.tools[0].selected);
    assert_eq!(found.tools[2].exit_code, Some(101));
    assert_eq!(found.tools[2].operation.as_deref(), Some("cargo test"));
    assert_eq!(found.tools[2].outcome, "nonzero_exit");
    assert_eq!(found.children.len(), 1);
    assert_eq!(found.children[0].name, "functions.exec_command");
    assert_eq!(found.children_complete, Some(true));
    assert!(!serde_json::to_string(&found).unwrap().contains("private"));
    assert!(
        !read(
            dir.path(),
            &loc,
            &links::Scope {
                turn: Some(tag("conflict")),
                ..Default::default()
            },
            &HashMap::new()
        )
        .unwrap()
        .matched
    );
    fs::write(&file, log.replacen(CHAT, "another-chat", 1)).unwrap();
    assert!(
        !read(dir.path(), &loc, &Default::default(), &HashMap::new())
            .unwrap()
            .matched
    );
}

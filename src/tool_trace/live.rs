//! Bounded live trace state, shared by receiver, NDJSON output, and dashboard.
use super::store::Store;
use std::sync::{Arc, Mutex};
pub type Shared = Arc<Mutex<State>>;
#[derive(Default)]
pub struct State {
    pub store: Store,
    pub enabled: bool,
    pub running: bool,
    pub last_received_ms: Option<i64>,
    pub accepted_batches: u64,
    pub rejected_batches: u64,
    pub ignored_records: u64,
    pub rejected_records: u64,
    pub last_rejection: Option<&'static str>,
}
impl State {
    pub fn snapshot(&self) -> serde_json::Value {
        let status = if !self.enabled {
            "disabled"
        } else if !self.running {
            "stopped"
        } else if let Some(last) = self.last_received_ms {
            if crate::mcp::time::now_ms() - last > 30_000 {
                "quiet"
            } else {
                "receiving"
            }
        } else {
            "waiting_for_exporter"
        };
        serde_json::json!({"schema_version":1,"kind":"trace_source","enabled":self.enabled,"status":status,
            "mode":"result_only","adapter":"codex-otlp-tool-result-v1","verified_client":"26.1111.21308 / codex-cli 0.169.0-alpha.11.1",
            "capabilities":{"starts":false,"nested_calls":"verified_railway","parallel_calls":"verified_railway","subagents":"unverified","cancellation":"unverified","tool_errors":"unverified","parent_calls":false},
            "attribution":"Exact verified Railway namespace; other plugin namespaces remain unidentified",
            "last_received_ms":self.last_received_ms,"accepted_batches":self.accepted_batches.to_string(),"rejected_batches":self.rejected_batches.to_string(),
            "last_rejection":self.last_rejection,"ignored_records":self.ignored_records.to_string(),"rejected_records":self.rejected_records.to_string(),"durable":false,"snapshot":self.store.snapshot()})
    }
}

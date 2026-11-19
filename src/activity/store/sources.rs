use crate::mcp::{collection::SourceView, discovery::RootView};
use serde::Serialize;

#[derive(Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sources {
    pub running: bool,
    pub demo: bool,
    pub roots: Vec<RootView>,
    pub files: Vec<LogFile>,
    pub discovery_limited: bool,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub rejected_files: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub open_failures: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub retired_sources: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub unsupported_records: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub ignored_records: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub projection_gaps: u64,
    pub trace: TraceSource,
}
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct TraceSource {
    pub enabled: bool,
    pub status: &'static str,
    pub last_received_ms: Option<i64>,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub rejected_batches: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub ignored_records: u64,
    pub result_only: bool,
}
impl Default for TraceSource {
    fn default() -> Self {
        Self {
            enabled: false,
            status: "disabled",
            last_received_ms: None,
            rejected_batches: 0,
            ignored_records: 0,
            result_only: true,
        }
    }
}
impl TraceSource {
    pub fn from_state(s: &crate::tool_trace::live::State) -> Self {
        Self {
            enabled: s.enabled,
            status: if !s.enabled {
                "disabled"
            } else if s.replay {
                "synthetic_replay"
            } else if !s.running {
                "stopped"
            } else if s.last_received_ms.is_none() {
                "waiting_for_exporter"
            } else if s
                .last_received_ms
                .is_some_and(|t| crate::mcp::time::now_ms() - t > 30_000)
            {
                "quiet"
            } else {
                "receiving"
            },
            last_received_ms: s.last_received_ms,
            rejected_batches: s.rejected_batches,
            ignored_records: s.ignored_records,
            result_only: !s.replay,
        }
    }
}

// Keep the legacy MCP wire format unchanged. Activity identifiers and cumulative
// counters use decimal strings, including values above JavaScript's safe integer.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct LogFile(#[serde(with = "ActivityFile")] pub SourceView);
#[derive(Serialize)]
#[serde(remote = "SourceView")]
struct ActivityFile {
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    id: u64,
    host: crate::mcp::model::Host,
    name: String,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    generation: u64,
    modified_ms: i64,
    status: &'static str,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    backlog_bytes: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    records: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    oversized: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    history_omitted_bytes: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    boundaries: u64,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_counters_preserve_full_precision_without_changing_mcp() {
        let source = SourceView {
            id: u64::MAX,
            host: crate::mcp::model::Host::CodexDesktop,
            name: "redacted.log".into(),
            generation: 1,
            modified_ms: 1,
            status: "following",
            backlog_bytes: 0,
            records: u64::MAX,
            oversized: 0,
            history_omitted_bytes: 0,
            boundaries: 0,
        };
        assert!(serde_json::to_value(&source).unwrap()["id"].is_number());
        let activity = serde_json::to_value(LogFile(source)).unwrap();
        assert_eq!(activity["id"], u64::MAX.to_string());
        assert_eq!(activity["records"], u64::MAX.to_string());
    }
}

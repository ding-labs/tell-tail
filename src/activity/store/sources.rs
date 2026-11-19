use crate::mcp::{collection::SourceView, discovery::RootView};
use serde::Serialize;

#[derive(Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sources {
    pub running: bool,
    pub demo: bool,
    pub roots: Vec<RootView>,
    pub files: Vec<SourceView>,
    pub discovery_limited: bool,
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

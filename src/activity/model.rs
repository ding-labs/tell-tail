use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
pub const ADAPTER: &str = "desktop-log-v1";
pub const MAX_ITEM_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Tools,
    Mcp,
    Connections,
    Desktop,
}
impl Category {
    pub fn key(self) -> &'static str {
        match self {
            Self::Tools => "tools",
            Self::Mcp => "mcp",
            Self::Connections => "connections",
            Self::Desktop => "desktop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// All fields are sanitized before entering this type. There is deliberately no
/// generic attributes map, raw body, URL, path, or native conversation identifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection: Option<std::sync::Arc<crate::tool_trace::inspection::Inspection>>,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub id: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub revision: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub ordinal: u64,
    pub category: Category,
    pub kind: &'static str,
    pub severity: Severity,
    pub source_level: &'static str,
    pub summary: String,
    pub subject: String,
    pub client: &'static str,
    pub source: &'static str,
    pub adapter: &'static str,
    pub source_ref: String,
    pub scope: &'static str,
    pub scope_ref: Option<String>,
    pub occurred_ms: Option<i64>,
    pub received_ms: i64,
    pub historical: bool,
    pub diagnostic: bool,
    pub outcome: &'static str,
    pub duration_ms: Option<u64>,
    pub duration_basis: Option<&'static str>,
    pub plugin: Option<String>,
    pub server: Option<String>,
    pub method: Option<&'static str>,
    pub attempt: Option<u32>,
    pub start_observed: bool,
    pub completion_observed: bool,
    pub execution_success: Option<bool>,
    pub tool_error: Option<bool>,
    pub gap_observed: bool,
    pub note: &'static str,
    pub advice: Option<&'static str>,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Evidence {
    pub source: String,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub generation: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub offset: u64,
}

impl Item {
    pub fn observation(category: Category, kind: &'static str, summary: &'static str) -> Self {
        Self {
            inspection: None,
            id: 0,
            revision: 0,
            ordinal: 0,
            category,
            kind,
            severity: Severity::Info,
            source_level: "info",
            summary: summary.into(),
            subject: "Desktop application".into(),
            client: "chatgpt_codex_desktop",
            source: "desktop_logs",
            adapter: ADAPTER,
            source_ref: String::new(),
            scope: "app_wide",
            scope_ref: None,
            occurred_ms: None,
            received_ms: 0,
            historical: false,
            diagnostic: false,
            outcome: "observed",
            duration_ms: None,
            duration_basis: None,
            plugin: None,
            server: None,
            method: None,
            attempt: None,
            start_observed: false,
            completion_observed: false,
            execution_success: None,
            tool_error: None,
            gap_observed: false,
            note: "A desktop observation; no verified chat link or complete cloud execution coverage.",
            advice: None,
            evidence: Vec::new(),
        }
    }
}

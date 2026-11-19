use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Started,
    Completed,
    Failed,
    Cancelled,
    Denied,
}

/// Input is intentionally not serializable: raw correlation identifiers must
/// never be sent to a browser, printed in diagnostics, or copied to an export.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub schema_version: u32,
    pub source: String,
    pub source_epoch: String,
    pub execution_id: String,
    pub call_id: Option<String>,
    pub event_id: Option<String>,
    pub tool: String,
    pub server: Option<String>,
    pub plugin: Option<String>,
    pub phase: Phase,
    pub timestamp_ms: Option<i64>,
    pub duration_ms: Option<u64>,
    pub execution_success: Option<bool>,
    pub tool_error: Option<bool>,
}

impl Observation {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err("Trace record exceeds the byte limit");
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| "Invalid trace record")?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("Unsupported trace schema");
        }
        let identifier =
            |s: &str| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        let label = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./:-".contains(&b))
        };
        if ![&self.source, &self.source_epoch, &self.execution_id]
            .into_iter()
            .all(|s| identifier(s))
            || [&self.call_id, &self.event_id]
                .into_iter()
                .any(|s| s.as_deref().is_some_and(|s| !identifier(s)))
            || !label(&self.tool)
            || [&self.server, &self.plugin]
                .into_iter()
                .any(|s| s.as_deref().is_some_and(|s| !label(s)))
        {
            return Err("Invalid trace identity or label");
        }
        if self
            .timestamp_ms
            .is_some_and(|n| !(0..=253_402_300_799_999).contains(&n))
            || self.duration_ms.is_some_and(|n| n > 86_400_000)
        {
            return Err("Invalid trace timing");
        }
        if self.phase == Phase::Started
            && (self.duration_ms.is_some()
                || self.execution_success.is_some()
                || self.tool_error.is_some())
        {
            return Err("A start cannot report an outcome");
        }
        if self.phase == Phase::Denied
            && (self.duration_ms.is_some()
                || self.execution_success.is_some()
                || self.tool_error.is_some())
        {
            return Err("A denied call cannot report an execution outcome");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Completed,
    Failed,
    Cancelled,
    Denied,
    Incomplete,
    Conflict,
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DurationBasis {
    SourceReported,
    SourceTimestamps,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct Call {
    #[serde(serialize_with = "decimal")]
    pub id: u64,
    #[serde(serialize_with = "decimal")]
    pub revision: u64,
    pub source: String,
    pub execution: String,
    pub correlated: bool,
    pub tool: String,
    pub server: Option<String>,
    pub plugin: Option<String>,
    pub status: Status,
    pub started_ms: Option<i64>,
    pub completed_ms: Option<i64>,
    pub received_ms: i64,
    pub duration_ms: Option<u64>,
    pub duration_basis: Option<DurationBasis>,
    pub execution_success: Option<bool>,
    pub tool_error: Option<bool>,
    pub start_observed: bool,
    pub completion_observed: bool,
    pub gap_observed: bool,
    pub historical: bool,
}

pub fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"source":"synthetic","source_epoch":"epoch",
            "execution_id":"PRIVATE_EXECUTION","call_id":"PRIVATE_CALL","tool":"docs.search",
            "plugin":"example","phase":"completed","timestamp_ms":100,"duration_ms":12,
            "execution_success":true,"tool_error":false})
    }
    #[test]
    fn contract_rejects_payloads_and_errors_never_echo_them() {
        let mut v = input();
        assert!(Observation::parse(&serde_json::to_vec(&v).unwrap()).is_ok());
        v["arguments"] = serde_json::json!({"password":"PRIVATE_PASSWORD"});
        let err = Observation::parse(&serde_json::to_vec(&v).unwrap())
            .err()
            .unwrap();
        assert_eq!(err, "Invalid trace record");
        assert!(!err.contains("PRIVATE"));
    }
    #[test]
    fn validates_versions_timings_and_labels() {
        for (key, value) in [
            ("schema_version", serde_json::json!(2)),
            ("tool", serde_json::json!("<script>")),
            ("duration_ms", serde_json::json!(u64::MAX)),
            ("timestamp_ms", serde_json::json!(-1)),
            ("phase", serde_json::json!("started")),
        ] {
            let mut v = input();
            v[key] = value;
            assert!(Observation::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        assert!(Observation::parse(&vec![b' '; MAX_RECORD_BYTES + 1]).is_err());
    }
}

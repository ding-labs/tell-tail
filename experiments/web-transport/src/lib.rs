//! Experimental frames, shared by both transports; not the production web schema.
use axum::extract::ws::Utf8Bytes;
use bytes::Bytes;
use clap::{Parser, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tell_tail::{cli::Args, engine::Engine, stats::State};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Profile {
    Representative,
    LargeCounters,
}

/// Capture the real engine after 60 completed seconds of varying input.
pub fn fixture(profile: Profile) -> Value {
    let args = Args::parse_from(["tell-tail"]);
    let mut engine = Engine::new(&args);
    for second in 0..60 {
        let lines = b"INFO connected\nWARN retry\nERROR timeout\nunknown record\n";
        engine.feed(
            &lines.repeat((second as usize % 7 + 1) * 123),
            Duration::from_secs(second),
        );
    }
    let mut snapshot = engine.snapshot(Duration::from_secs(60), State::Following, Some(0));
    snapshot.session_id = "transport-benchmark".into();
    snapshot.observed_at_unix_seconds = 0.0;
    snapshot.source = "benchmark.log".into();
    if matches!(profile, Profile::LargeCounters) {
        // Deliberate width stress, not an internally consistent ingestion ledger.
        snapshot.totals.records = u64::MAX;
        snapshot.totals.levels.0.fill(u64::MAX / 7);
        snapshot.totals.parse_failures = u64::MAX;
        snapshot.totals.oversized_records = u64::MAX;
        snapshot.bytes_read = u64::MAX;
        snapshot.history.fill(u64::MAX);
    }
    metrics(serde_json::to_value(snapshot).expect("finite fixture"))
}

/// Use decimal strings for counters so the renderer can inspect exact u64 values.
/// This is benchmark-only conversion, not a commitment to a shipping API.
pub fn metrics(mut value: Value) -> Value {
    fn convert(value: &mut Value, key: &str) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    convert(value, key);
                }
            }
            Value::Array(values) => {
                for value in values {
                    convert(value, key);
                }
            }
            Value::Number(n)
                if n.is_u64() && !matches!(key, "schema_version" | "configured_seconds") =>
            {
                *value = Value::String(n.to_string());
            }
            _ => (),
        }
    }
    value
        .as_object_mut()
        .expect("snapshot object")
        .remove("schema_version");
    convert(&mut value, "");
    value
}

pub struct Frame {
    pub sequence: u64,
    pub json: Utf8Bytes,
    pub sse: Bytes,
}

impl Frame {
    /// Build JSON and SSE framing once, before fanout to any client.
    pub fn new(sequence: u64, published_us: u64, metrics: &Value) -> Self {
        #[derive(Serialize)]
        struct Envelope<'a> {
            benchmark_schema: u32,
            sequence: String,
            published_us: u64,
            metrics: &'a Value,
        }
        let json = serde_json::to_string(&Envelope {
            benchmark_schema: 1,
            sequence: sequence.to_string(),
            published_us,
            metrics,
        })
        .expect("finite metrics");
        let sse = Bytes::from(format!("event: snapshot\nid: {sequence}\ndata: {json}\n\n"));
        Self {
            sequence,
            json: json.into(),
            sse,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_completed_history_and_preserves_large_counters() {
        let normal = fixture(Profile::Representative);
        assert_eq!(normal["history"].as_array().unwrap().len(), 60);
        assert_eq!(normal["history"][0], "492");
        assert_eq!(normal["window"]["configured_seconds"], 60);
        assert!(normal["window"]["records_per_second"].is_f64());
        assert!(normal.get("schema_version").is_none());
        let large = fixture(Profile::LargeCounters);
        assert_eq!(large["totals"]["records"], u64::MAX.to_string());
        assert_eq!(large["history"][59], u64::MAX.to_string());
    }

    #[test]
    fn both_transports_carry_identical_json_without_line_injection() {
        let frame = Frame::new(7, 123, &serde_json::json!({"source": "x\ndata: bad"}));
        let sse = std::str::from_utf8(&frame.sse).unwrap();
        let data = sse.strip_prefix("event: snapshot\nid: 7\ndata: ").unwrap();
        assert_eq!(data.strip_suffix("\n\n").unwrap(), frame.json.as_str());
        assert_eq!(sse.lines().count(), 4);
    }
}

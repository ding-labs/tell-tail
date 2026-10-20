//! Benchmark metadata wrapping production web frames, shared by both transports.
use axum::extract::ws::Utf8Bytes;
use bytes::Bytes;
use clap::{Parser, ValueEnum};
use std::{io, time::Duration};
use tell_tail::{
    cli::Args,
    engine::Engine,
    stats::{Snapshot, State},
    web_frame::{MAX_FRAME_BYTES, WebFrame},
};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Profile {
    Representative,
    LargeCounters,
}

/// Capture the real engine after 60 completed seconds of varying input.
pub fn fixture(profile: Profile) -> Snapshot {
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
    snapshot
}

pub struct Frame {
    pub sequence: u64,
    pub json: Utf8Bytes,
    pub sse: Bytes,
}

impl Frame {
    /// Build JSON and SSE framing once, before fanout to any client.
    pub fn new(
        sequence: u64,
        published_us: u64,
        snapshot: &Snapshot,
        padding: u32,
    ) -> io::Result<Self> {
        let frame = WebFrame::encode(sequence, snapshot)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        // Copy the already encoded JSON into a benchmark-only wrapper. No metrics
        // conversion, second metrics serialization, or metadata in the web schema.
        let prefix = format!(
            "{{\"benchmark_schema\":2,\"published_us\":{published_us},\"stress_padding\":\""
        );
        let middle = "\",\"frame\":";
        let size = prefix.len() + padding as usize + middle.len() + frame.json().len() + 1;
        if size > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "benchmark envelope exceeds 65536 JSON bytes",
            ));
        }
        let mut json = String::with_capacity(size);
        json.push_str(&prefix);
        json.extend(std::iter::repeat_n('x', padding as usize));
        json.push_str(middle);
        json.push_str(frame.json());
        json.push('}');
        let sse = Bytes::from(format!(
            "event: snapshot\nid: {}\ndata: {json}\n\n",
            frame.event_id()
        ));
        Ok(Self {
            sequence,
            json: json.into(),
            sse,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn captures_completed_history_and_preserves_large_counters() {
        let value = |profile| {
            let frame = WebFrame::encode(0, &fixture(profile)).unwrap();
            serde_json::from_str::<Value>(frame.json()).unwrap()["metrics"].clone()
        };
        let normal = value(Profile::Representative);
        assert_eq!(normal["history"].as_array().unwrap().len(), 60);
        assert_eq!(normal["history"][0], "492");
        assert_eq!(normal["window"]["configured_seconds"], 60);
        assert!(normal["window"]["records_per_second"].is_f64());
        assert!(normal.get("schema_version").is_none());
        let large = value(Profile::LargeCounters);
        assert_eq!(large["totals"]["records"], u64::MAX.to_string());
        assert_eq!(large["history"][59], u64::MAX.to_string());
    }

    #[test]
    fn both_transports_carry_identical_json_without_line_injection() {
        let mut snapshot = fixture(Profile::Representative);
        snapshot.source = "x\ndata: bad".into();
        let frame = Frame::new(7, 123, &snapshot, 0).unwrap();
        let sse = std::str::from_utf8(&frame.sse).unwrap();
        let data = sse
            .strip_prefix("event: snapshot\nid: transport-benchmark:7\ndata: ")
            .unwrap();
        assert_eq!(data.strip_suffix("\n\n").unwrap(), frame.json.as_str());
        assert_eq!(sse.lines().count(), 4);
        let envelope: Value = serde_json::from_str(frame.json.as_str()).unwrap();
        assert_eq!(envelope["benchmark_schema"], 2);
        assert_eq!(envelope["published_us"], 123);
        assert_eq!(envelope["frame"]["web_schema_version"], 1);
        assert_eq!(envelope["frame"]["sequence"], "7");
        assert_eq!(envelope["frame"]["metrics"]["source"], snapshot.source);
        assert_eq!(envelope["frame"].as_object().unwrap().len(), 3);
    }

    #[test]
    fn padding_and_production_encoding_errors_are_fallible() {
        let mut snapshot = fixture(Profile::Representative);
        assert!(Frame::new(0, 0, &snapshot, 60_000).unwrap().json.len() <= MAX_FRAME_BYTES);
        assert!(Frame::new(0, 0, &snapshot, u32::MAX).is_err());
        snapshot.source = "x".repeat(MAX_FRAME_BYTES);
        assert!(Frame::new(0, 0, &snapshot, 0).is_err());
    }
}

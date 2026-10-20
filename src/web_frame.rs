//! Version 1 web frames, independent of HTTP and async runtimes.
//!
//! Encode an owned publication after releasing its snapshot lock, then share the
//! resulting immutable frame. Sequence numbers are native publication generations
//! (including zero); gaps are expected when a subscriber coalesces publications.
use crate::{
    parse::Level,
    stats::{Counts, Levels, Snapshot, State, Window},
};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::{fmt, io};

pub const WEB_SCHEMA_VERSION: u32 = 1;
/// Maximum UTF-8 JSON payload size, excluding transport framing.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// A complete, bounded JSON payload ready to share between consumers.
#[derive(Debug)]
pub struct WebFrame {
    sequence: u64,
    event_id: Box<str>,
    json: Box<str>,
}

impl WebFrame {
    /// Encode without cloning the snapshot or allocating an unbounded JSON value.
    /// Errors must fail the publisher; retaining an older frame would hide failure.
    pub fn encode(sequence: u64, snapshot: &Snapshot) -> Result<Self, EncodeError> {
        if snapshot.session_id.is_empty() || snapshot.session_id.contains(['\r', '\n', '\0']) {
            return Err(EncodeError::InvalidSessionId);
        }
        if snapshot.history.len() > 60 {
            return Err(EncodeError::InvalidHistory);
        }
        let mut writer = FrameWriter {
            bytes: Vec::with_capacity(MAX_FRAME_BYTES),
            too_large: false,
        };
        let result = serde_json::to_writer(
            &mut writer,
            &Envelope {
                web_schema_version: WEB_SCHEMA_VERSION,
                sequence: Decimal(sequence),
                metrics: Metrics::from(snapshot),
            },
        );
        if writer.too_large {
            return Err(EncodeError::TooLarge);
        }
        result.map_err(EncodeError::Serialization)?;
        Ok(Self {
            sequence,
            event_id: format!("{}:{sequence}", snapshot.session_id).into_boxed_str(),
            // serde_json always emits UTF-8, with embedded newlines escaped.
            json: String::from_utf8(writer.bytes)
                .expect("JSON is UTF-8")
                .into_boxed_str(),
        })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// SSE event ID; safe to place on one `id:` line. Reconnects need no replay.
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn json(&self) -> &str {
        &self.json
    }
}

#[derive(Debug)]
pub enum EncodeError {
    TooLarge,
    InvalidSessionId,
    InvalidHistory,
    Serialization(serde_json::Error),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("web frame exceeds 65536 JSON bytes"),
            Self::InvalidSessionId => {
                f.write_str("web session ID is empty or contains CR, LF, or NUL")
            }
            Self::InvalidHistory => f.write_str("web history exceeds 60 completed seconds"),
            Self::Serialization(error) => write!(f, "cannot encode web frame: {error}"),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}

struct FrameWriter {
    bytes: Vec<u8>,
    too_large: bool,
}

impl io::Write for FrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_FRAME_BYTES - self.bytes.len() {
            self.too_large = true;
            return Err(io::Error::other("web frame size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Decimal(u64);
impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

struct Finite(f64);
impl Serialize for Finite {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !self.0.is_finite() {
            return Err(serde::ser::Error::custom("non-finite web metric"));
        }
        serializer.serialize_f64(self.0)
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    web_schema_version: u32,
    sequence: Decimal,
    metrics: Metrics<'a>,
}

#[derive(Serialize)]
struct Metrics<'a> {
    kind: &'a str,
    session_id: &'a str,
    source: &'a str,
    state: State,
    observed_at_unix_seconds: Finite,
    elapsed_seconds: Finite,
    totals: WebCounts<'a>,
    bytes_read: Decimal,
    pending_bytes: Decimal,
    skipped_bytes: Decimal,
    discarded_partial_records: Decimal,
    backlog_bytes: Option<Decimal>,
    warning_ratio: Option<Finite>,
    error_ratio: Option<Finite>,
    window: WebWindow<'a>,
    history: History<'a>,
}

impl<'a> From<&'a Snapshot> for Metrics<'a> {
    fn from(snapshot: &'a Snapshot) -> Self {
        Self {
            kind: snapshot.kind,
            session_id: &snapshot.session_id,
            source: &snapshot.source,
            state: snapshot.state,
            observed_at_unix_seconds: Finite(snapshot.observed_at_unix_seconds),
            elapsed_seconds: Finite(snapshot.elapsed_seconds),
            totals: WebCounts::from(&snapshot.totals),
            bytes_read: Decimal(snapshot.bytes_read),
            pending_bytes: Decimal(snapshot.pending_bytes),
            skipped_bytes: Decimal(snapshot.skipped_bytes),
            discarded_partial_records: Decimal(snapshot.discarded_partial_records),
            backlog_bytes: snapshot.backlog_bytes.map(Decimal),
            warning_ratio: snapshot.warning_ratio.map(Finite),
            error_ratio: snapshot.error_ratio.map(Finite),
            window: WebWindow::from(&snapshot.window),
            history: History(&snapshot.history),
        }
    }
}

#[derive(Serialize)]
struct WebCounts<'a> {
    records: Decimal,
    levels: WebLevels<'a>,
    parse_failures: Decimal,
    oversized_records: Decimal,
}

impl<'a> From<&'a Counts> for WebCounts<'a> {
    fn from(counts: &'a Counts) -> Self {
        Self {
            records: Decimal(counts.records),
            levels: WebLevels(&counts.levels),
            parse_failures: Decimal(counts.parse_failures),
            oversized_records: Decimal(counts.oversized_records),
        }
    }
}

struct WebLevels<'a>(&'a Levels);
impl Serialize for WebLevels<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(Level::ALL.len()))?;
        for level in Level::ALL {
            map.serialize_entry(level.name(), &Decimal(self.0.get(level)))?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct WebWindow<'a> {
    configured_seconds: u32,
    resolution_seconds: Finite,
    observed_seconds: Finite,
    counts: WebCounts<'a>,
    bytes_read: Decimal,
    records_per_second: Finite,
    bytes_per_second: Finite,
    warning_ratio: Option<Finite>,
    error_ratio: Option<Finite>,
}

impl<'a> From<&'a Window> for WebWindow<'a> {
    fn from(window: &'a Window) -> Self {
        Self {
            configured_seconds: window.configured_seconds,
            resolution_seconds: Finite(window.resolution_seconds),
            observed_seconds: Finite(window.observed_seconds),
            counts: WebCounts::from(&window.counts),
            bytes_read: Decimal(window.bytes_read),
            records_per_second: Finite(window.records_per_second),
            bytes_per_second: Finite(window.bytes_per_second),
            warning_ratio: window.warning_ratio.map(Finite),
            error_ratio: window.error_ratio.map(Finite),
        }
    }
}

struct History<'a>(&'a [u64]);
impl Serialize for History<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().copied().map(Decimal))
    }
}

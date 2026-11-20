//! Aggregate complete records from the existing follower, without retaining bodies.
use crate::{
    cli::Format,
    parse::Classifier,
    stats::{Snapshot, State, Stats},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub type Shared = Arc<Mutex<LogMetrics>>;
pub struct LogMetrics {
    stats: Stats,
    classifier: Classifier,
    started: Instant,
    ended: Option<Duration>,
    backlog: u64,
    sources: usize,
}
impl LogMetrics {
    pub fn new(demo: bool) -> Self {
        Self {
            stats: Stats::new(
                5,
                if demo {
                    "Desktop logs · synthetic records"
                } else {
                    "Desktop logs · aggregate record contents"
                }
                .into(),
            ),
            classifier: Classifier::new(Format::Auto, "level".into()),
            started: Instant::now(),
            ended: None,
            backlog: 0,
            sources: 0,
        }
    }
    pub fn record(&mut self, bytes: &[u8]) {
        let at = self.started.elapsed();
        self.stats.add_bytes(bytes.len() as u64, at);
        self.stats
            .record(self.classifier.classify(bytes), false, at);
    }
    pub fn update(&mut self, sources: &[super::collection::SourceView], running: bool) {
        self.sources = sources.len();
        self.backlog = sources.iter().map(|s| s.backlog_bytes).sum();
        if !running && self.ended.is_none() {
            self.ended = Some(self.started.elapsed());
        }
    }
    pub fn snapshot(&mut self) -> Snapshot {
        let state = if self.ended.is_some() {
            State::Stopped
        } else if self.sources == 0 {
            State::Waiting
        } else {
            State::Following
        };
        self.stats.snapshot(
            self.ended.unwrap_or_else(|| self.started.elapsed()),
            state,
            0,
            Some(self.backlog),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_complete_records_without_storing_private_previews() {
        let mut m = LogMetrics::new(false);
        m.record(b"2026-11-19T12:00:00Z error PRIVATE_PROMPT");
        m.record(b"WARN PRIVATE_SECRET");
        let s = m.snapshot();
        assert_eq!(s.totals.records, 2);
        assert_eq!(s.totals.levels.get(crate::parse::Level::Error), 1);
        assert_eq!(s.totals.levels.get(crate::parse::Level::Warn), 1);
        assert!(s.recent_errors.is_empty());
        assert!(!serde_json::to_string(&s).unwrap().contains("PRIVATE"));
        m.update(&[], false);
        assert_eq!(m.snapshot().state, State::Stopped);
        assert_eq!(m.snapshot().elapsed_seconds, m.snapshot().elapsed_seconds);
    }
}

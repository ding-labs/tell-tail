use crate::parse::{Classification, Level};
use serde::{Serialize, Serializer, ser::SerializeMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Levels(pub [u64; 7]);
impl Levels {
    pub fn total(&self) -> u64 {
        self.0.iter().sum()
    }
    pub fn get(&self, level: Level) -> u64 {
        self.0[level as usize]
    }
    pub fn warning_ratio(&self) -> Option<f64> {
        self.ratio(self.get(Level::Warn))
    }
    pub fn error_ratio(&self) -> Option<f64> {
        self.ratio(self.get(Level::Error) + self.get(Level::Fatal))
    }
    fn ratio(&self, count: u64) -> Option<f64> {
        (self.total() != 0).then(|| count as f64 / self.total() as f64)
    }
    fn merge(&mut self, other: Self) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a += b;
        }
    }
}
impl Serialize for Levels {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(7))?;
        for level in Level::ALL {
            map.serialize_entry(level.name(), &self.get(level))?;
        }
        map.end()
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Counts {
    pub records: u64,
    pub levels: Levels,
    pub parse_failures: u64,
    pub oversized_records: u64,
}
impl Counts {
    fn record(&mut self, classification: Classification, oversized: bool) {
        self.records += 1;
        self.levels.0[classification.level as usize] += 1;
        self.parse_failures += u64::from(classification.parse_failure);
        self.oversized_records += u64::from(oversized);
    }
    fn merge(&mut self, other: Self) {
        self.records += other.records;
        self.levels.merge(other.levels);
        self.parse_failures += other.parse_failures;
        self.oversized_records += other.oversized_records;
    }
    fn subtract(&mut self, other: Self) {
        self.records -= other.records;
        self.parse_failures -= other.parse_failures;
        self.oversized_records -= other.oversized_records;
        for (a, b) in self.levels.0.iter_mut().zip(other.levels.0) {
            *a -= b;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Reading,
    Following,
    Waiting,
    Complete,
    Stopped,
    Failed,
}
impl State {
    pub fn is_final(self) -> bool {
        matches!(self, Self::Complete | Self::Stopped | Self::Failed)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Window {
    pub configured_seconds: u32,
    pub resolution_seconds: f64,
    pub observed_seconds: f64,
    pub counts: Counts,
    pub bytes_read: u64,
    pub records_per_second: f64,
    pub bytes_per_second: f64,
    pub warning_ratio: Option<f64>,
    pub error_ratio: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub kind: &'static str,
    pub session_id: String,
    pub source: String,
    pub state: State,
    pub observed_at_unix_seconds: f64,
    pub elapsed_seconds: f64,
    pub totals: Counts,
    pub bytes_read: u64,
    pub pending_bytes: u64,
    pub skipped_bytes: u64,
    pub discarded_partial_records: u64,
    pub backlog_bytes: Option<u64>,
    pub warning_ratio: Option<f64>,
    pub error_ratio: Option<f64>,
    pub window: Window,
    /// Completed per-second record counts, oldest first, at most 60 points.
    pub history: Vec<u64>,
    /// Dashboard-only previews; the CLI JSON and webhook contracts omit logs.
    #[serde(skip)]
    pub recent_errors: Vec<crate::recent::RecentError>,
}
#[derive(Clone, Copy, Default)]
struct Bucket {
    counts: Counts,
    bytes: u64,
}
#[derive(Clone, Copy, Default)]
struct HistoryBucket {
    second: u64,
    records: u64,
}
const TICKS_PER_SECOND: u64 = 10;
pub struct Stats {
    pub totals: Counts,
    pub bytes_read: u64,
    pub skipped_bytes: u64,
    pub discarded_partial_records: u64,
    buckets: Vec<Bucket>,
    tick: u64,
    closed_counts: Counts,
    closed_bytes: u64,
    history: [HistoryBucket; 61],
    window: u32,
    source: String,
    session_id: String,
}
impl Stats {
    pub fn new(window: u32, source: String) -> Self {
        assert!((1..=3600).contains(&window));
        let session_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        Self {
            totals: Counts::default(),
            bytes_read: 0,
            skipped_bytes: 0,
            discarded_partial_records: 0,
            buckets: vec![Bucket::default(); window as usize * TICKS_PER_SECOND as usize + 1],
            tick: 0,
            closed_counts: Counts::default(),
            closed_bytes: 0,
            history: [HistoryBucket::default(); 61],
            window,
            source,
            session_id,
        }
    }
    // Time advances once per input batch or snapshot. The aggregate excludes the
    // current bucket, avoiding another counter update for every individual log.
    pub(crate) fn advance(&mut self, at: Duration) {
        let tick = at
            .as_secs()
            .saturating_mul(TICKS_PER_SECOND)
            .saturating_add(u64::from(at.subsec_millis()) / 100);
        assert!(tick >= self.tick, "processing time must be monotonic");
        if tick == self.tick {
            return;
        }
        if tick - self.tick >= self.buckets.len() as u64 {
            self.buckets.fill(Bucket::default());
            self.closed_counts = Counts::default();
            self.closed_bytes = 0;
        } else {
            let old = self.buckets[self.tick as usize % self.buckets.len()];
            self.closed_counts.merge(old.counts);
            self.closed_bytes += old.bytes;
            for next in self.tick + 1..=tick {
                let index = next as usize % self.buckets.len();
                let expired = std::mem::take(&mut self.buckets[index]);
                self.closed_counts.subtract(expired.counts);
                self.closed_bytes -= expired.bytes;
            }
        }
        self.tick = tick;
    }
    pub fn add_bytes(&mut self, bytes: u64, at: Duration) {
        self.advance(at);
        self.bytes_read += bytes;
        let index = self.tick as usize % self.buckets.len();
        self.buckets[index].bytes += bytes;
    }
    pub fn record(&mut self, class: Classification, oversized: bool, at: Duration) {
        self.advance(at);
        self.record_current(class, oversized);
    }
    pub(crate) fn record_current(&mut self, class: Classification, oversized: bool) {
        self.totals.record(class, oversized);
        let index = self.tick as usize % self.buckets.len();
        self.buckets[index].counts.record(class, oversized);
        let second = self.tick / TICKS_PER_SECOND;
        let history = &mut self.history[second as usize % 61];
        if history.second != second {
            *history = HistoryBucket { second, records: 0 };
        }
        history.records += 1;
    }
    pub fn snapshot(
        &mut self,
        at: Duration,
        state: State,
        pending_bytes: u64,
        backlog_bytes: Option<u64>,
    ) -> Snapshot {
        self.advance(at);
        let first_tick = self
            .tick
            .saturating_sub(u64::from(self.window) * TICKS_PER_SECOND);
        let start = Duration::from_secs(first_tick / TICKS_PER_SECOND)
            + Duration::from_millis((first_tick % TICKS_PER_SECOND) * 100);
        let duration = at.saturating_sub(start).as_secs_f64();
        let current = self.buckets[self.tick as usize % self.buckets.len()];
        let mut counts = self.closed_counts;
        counts.merge(current.counts);
        let bytes = self.closed_bytes + current.bytes;
        let now = at.as_secs();
        let history = (now.saturating_sub(60)..now)
            .map(|s| {
                let b = self.history[s as usize % 61];
                if b.second == s { b.records } else { 0 }
            })
            .collect();
        Snapshot {
            schema_version: 2,
            kind: if state.is_final() {
                "final"
            } else {
                "snapshot"
            },
            session_id: self.session_id.clone(),
            source: self.source.clone(),
            state,
            observed_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64(),
            elapsed_seconds: at.as_secs_f64(),
            totals: self.totals,
            bytes_read: self.bytes_read,
            pending_bytes,
            skipped_bytes: self.skipped_bytes,
            discarded_partial_records: self.discarded_partial_records,
            backlog_bytes,
            warning_ratio: self.totals.levels.warning_ratio(),
            error_ratio: self.totals.levels.error_ratio(),
            window: Window {
                configured_seconds: self.window,
                resolution_seconds: 0.1,
                observed_seconds: duration,
                counts,
                bytes_read: bytes,
                records_per_second: rate(counts.records, duration),
                bytes_per_second: rate(bytes, duration),
                warning_ratio: counts.levels.warning_ratio(),
                error_ratio: counts.levels.error_ratio(),
            },
            history,
            recent_errors: Vec::new(),
        }
    }
}
fn rate(count: u64, seconds: f64) -> f64 {
    if seconds > 0.0 {
        count as f64 / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(s: &mut Stats, second: f64, level: Level) {
        s.record(
            Classification {
                level,
                parse_failure: false,
            },
            false,
            Duration::from_secs_f64(second),
        );
    }
    #[test]
    fn window_expires_on_idle_and_preserves_totals() {
        let mut s = Stats::new(2, "test".into());
        record(&mut s, 0.1, Level::Info);
        record(&mut s, 1.1, Level::Warn);
        record(&mut s, 2.1, Level::Fatal);
        let a = s.snapshot(Duration::from_millis(2500), State::Reading, 0, None);
        assert_eq!(a.window.counts.records, 2);
        assert_eq!(a.window.observed_seconds, 2.0);
        assert_eq!(a.window.error_ratio, Some(0.5));
        assert_eq!(a.totals.records, 3);
        assert_eq!(a.totals.records, a.totals.levels.total());
        let idle = s.snapshot(Duration::from_secs(100), State::Following, 0, Some(0));
        assert_eq!(idle.window.counts.records, 0);
        assert_eq!(idle.window.records_per_second, 0.0);
        assert_eq!(idle.window.error_ratio, None);
        assert_eq!(idle.totals.records, 3);
    }
    #[test]
    fn startup_and_ring_reuse_have_defined_rates() {
        let mut s = Stats::new(1, "test".into());
        assert_eq!(
            s.snapshot(Duration::ZERO, State::Reading, 0, None)
                .window
                .records_per_second,
            0.0
        );
        for i in 0..1000 {
            record(&mut s, i as f64 + 0.5, Level::Unknown);
        }
        let a = s.snapshot(Duration::from_millis(999500), State::Complete, 4, None);
        assert_eq!(a.window.counts.records, 2);
        assert_eq!(a.window.records_per_second, 2.0);
        assert_eq!(a.history, vec![1; 60]);
        assert_eq!(a.totals.records, 1000);
        assert_eq!(a.warning_ratio, Some(0.0));
        assert_eq!(a.kind, "final");
        assert_eq!(s.buckets.len(), 11);
    }
    #[test]
    fn quantized_window_matches_event_ledger_across_boundaries_and_jumps() {
        for window in [1, 60, 3600] {
            let mut stats = Stats::new(window, "ledger".into());
            let mut ledger = Vec::new();
            for (index, ms) in [
                0, 1, 99, 100, 950, 999, 1000, 1001, 1999, 2000, 60_099, 60_100, 3_600_000,
                3_600_099, 7_200_001,
            ]
            .into_iter()
            .enumerate()
            {
                let at = Duration::from_millis(ms);
                let level = Level::ALL[index % 7];
                let bytes = index as u64 + 5;
                stats.add_bytes(bytes, at);
                stats.record(
                    Classification {
                        level,
                        parse_failure: false,
                    },
                    false,
                    at,
                );
                ledger.push((ms, level, bytes));
                let report = stats.snapshot(at, State::Reading, 0, None);
                let start = ms.saturating_sub(u64::from(window) * 1000) / 100 * 100;
                let mut levels = Levels::default();
                let mut byte_count = 0;
                for &(_, level, bytes) in ledger.iter().filter(|(time, _, _)| *time >= start) {
                    levels.0[level as usize] += 1;
                    byte_count += bytes;
                }
                assert_eq!(report.window.counts.levels, levels);
                assert_eq!(report.window.counts.records, levels.total());
                assert_eq!(report.window.bytes_read, byte_count);
                assert_eq!(report.window.observed_seconds, (ms - start) as f64 / 1000.0);
                assert_eq!(report.totals.records, ledger.len() as u64);
                assert_eq!(report.schema_version, 2);
                assert_eq!(report.window.resolution_seconds, 0.1);
            }
        }
    }
    #[test]
    fn one_second_window_survives_the_second_boundary() {
        let mut stats = Stats::new(1, "boundary".into());
        record(&mut stats, 0.95, Level::Warn);
        for ms in [999, 1000, 1999] {
            let s = stats.snapshot(Duration::from_millis(ms), State::Following, 0, None);
            assert_eq!(s.window.counts.records, 1);
            assert_eq!(s.window.warning_ratio, Some(1.0));
        }
        let s = stats.snapshot(Duration::from_millis(2000), State::Following, 0, None);
        assert_eq!(s.window.counts.records, 0);
        assert_eq!(s.window.warning_ratio, None);
        assert_eq!(s.window.observed_seconds, 1.0);
        assert_eq!(s.totals.records, 1);
        assert_eq!(s.history, vec![1, 0]);
    }
}

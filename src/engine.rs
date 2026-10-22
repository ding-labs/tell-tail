use crate::{
    cli::{Args, Output},
    framing::Framer,
    parse::{Classification, Classifier},
    recent::RecentErrors,
    stats::{Snapshot, State, Stats},
};
use std::{
    io::{self, Read},
    time::{Duration, Instant},
};

pub struct Engine {
    pub stats: Stats,
    framer: Framer,
    classifier: Classifier,
    recent_errors: Option<RecentErrors>,
}
impl Engine {
    pub fn new(args: &Args) -> Self {
        let review = args.output == Output::Tui;
        #[cfg(feature = "web")]
        let review = review || args.output == Output::Web;
        Self {
            stats: Stats::new(
                args.window,
                if args.apple_unified {
                    "macOS unified log".into()
                } else {
                    args.path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "stdin".into())
                },
            ),
            framer: Framer::new(args.max_record_bytes as usize),
            classifier: Classifier::new(args.input_format(), args.level_field.clone()),
            recent_errors: review.then(RecentErrors::default),
        }
    }
    pub fn feed(&mut self, bytes: &[u8], at: Duration) {
        self.stats.add_bytes(bytes.len() as u64, at);
        let Self {
            stats,
            framer,
            classifier,
            recent_errors,
        } = self;
        framer.feed_counted(bytes, |line, oversized, consumed| {
            let class = if oversized {
                Some(Classification::unknown())
            } else {
                classifier.classify_record(line)
            };
            if let Some(class) = class {
                stats.record_current(class, oversized);
                if let Some(recent) = recent_errors {
                    recent.record(class.level, stats.totals.records, at, line);
                }
            } else {
                stats.skipped_bytes += consumed;
            }
        });
    }
    pub fn finish(&mut self, at: Duration) {
        self.stats.advance(at);
        let Self {
            stats,
            framer,
            classifier,
            recent_errors,
        } = self;
        framer.finish_counted(|line, oversized, consumed| {
            let class = if oversized {
                Some(Classification::unknown())
            } else {
                classifier.classify_record(line)
            };
            if let Some(class) = class {
                stats.record_current(class, oversized);
                if let Some(recent) = recent_errors {
                    recent.record(class.level, stats.totals.records, at, line);
                }
            } else {
                stats.skipped_bytes += consumed;
            }
        });
    }
    pub fn discard_partial(&mut self) {
        let n = self.framer.pending_bytes();
        if n > 0 {
            self.stats.discarded_partial_records += 1;
            self.stats.skipped_bytes += n;
        }
        self.framer.clear();
    }
    pub fn snapshot(&mut self, at: Duration, state: State, backlog: Option<u64>) -> Snapshot {
        let mut snapshot = self
            .stats
            .snapshot(at, state, self.framer.pending_bytes(), backlog);
        if let Some(recent) = &self.recent_errors {
            snapshot.recent_errors = recent.snapshot();
        }
        snapshot
    }
}
pub fn scan(mut reader: impl Read, args: &Args) -> io::Result<Snapshot> {
    let mut engine = Engine::new(args);
    let started = Instant::now();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let n = match reader.read(&mut buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            break;
        }
        engine.feed(&buffer[..n], started.elapsed());
    }
    engine.finish(started.elapsed());
    Ok(engine.snapshot(started.elapsed(), State::Complete, args.path().map(|_| 0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Level;
    use clap::Parser;

    #[test]
    fn dashboard_review_follows_classification_chunking_and_final_records() {
        let input =
            b"INFO normal\nWARN ignored\nERROR first\n{\"level\":\"panic\",\"message\":\"last\"}";
        for chunk in 1..=input.len() {
            let mut engine = Engine::new(&Args::parse_from(["tell-tail", "--output", "tui"]));
            for bytes in input.chunks(chunk) {
                engine.feed(bytes, Duration::from_secs(1));
            }
            let frozen = engine.snapshot(Duration::from_secs(1), State::Following, None);
            assert_eq!(frozen.recent_errors.len(), 1);
            engine.finish(Duration::from_secs(2));
            let final_snapshot = engine.snapshot(Duration::from_secs(2), State::Complete, None);
            assert_eq!(final_snapshot.totals.records, 4);
            assert_eq!(final_snapshot.recent_errors.len(), 2);
            assert_eq!(final_snapshot.recent_errors[0].level, Level::Fatal);
            assert_eq!(final_snapshot.recent_errors[0].record, 4);
            assert_eq!(final_snapshot.recent_errors[0].elapsed_seconds, 2.0);
            assert_eq!(
                final_snapshot.recent_errors[1].preview.as_ref(),
                "ERROR first"
            );
            assert_eq!(frozen.recent_errors.len(), 1);
            assert!(
                serde_json::to_value(&final_snapshot)
                    .unwrap()
                    .get("recent_errors")
                    .is_none()
            );
        }
    }

    #[test]
    fn machine_outputs_do_not_retain_previews() {
        for output in ["auto", "json", "text", "prometheus"] {
            let mut engine = Engine::new(&Args::parse_from(["tell-tail", "--output", output]));
            engine.feed(b"FATAL secret\nERROR another\n", Duration::ZERO);
            assert!(
                engine
                    .snapshot(Duration::ZERO, State::Complete, None)
                    .recent_errors
                    .is_empty()
            );
        }
    }
}

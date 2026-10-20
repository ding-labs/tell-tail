use crate::{
    cli::Args,
    framing::Framer,
    parse::{Classification, Classifier},
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
}
impl Engine {
    pub fn new(args: &Args) -> Self {
        Self {
            stats: Stats::new(
                args.window,
                args.path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "stdin".into()),
            ),
            framer: Framer::new(args.max_record_bytes as usize),
            classifier: Classifier::new(args.format, args.level_field.clone()),
        }
    }
    pub fn feed(&mut self, bytes: &[u8], at: Duration) {
        self.stats.add_bytes(bytes.len() as u64, at);
        let Self {
            stats,
            framer,
            classifier,
        } = self;
        framer.feed_counted(bytes, |line, oversized, consumed| {
            let class = if oversized {
                Some(Classification::unknown())
            } else {
                classifier.classify_record(line)
            };
            if let Some(class) = class {
                stats.record_current(class, oversized);
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
        } = self;
        framer.finish_counted(|line, oversized, consumed| {
            let class = if oversized {
                Some(Classification::unknown())
            } else {
                classifier.classify_record(line)
            };
            if let Some(class) = class {
                stats.record_current(class, oversized);
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
        self.stats
            .snapshot(at, state, self.framer.pending_bytes(), backlog)
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

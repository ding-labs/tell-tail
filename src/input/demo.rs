//! A bounded, offline input for exploring the real ingestion and dashboard paths.
use super::{Event, State};
use std::{
    io, thread,
    time::{Duration, Instant},
};

pub struct Demo {
    started: Instant,
    next_at: Duration,
    slot: u8,
    pending: &'static [u8],
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            next_at: Duration::ZERO,
            slot: 0,
            pending: &[],
        }
    }

    pub fn next(&mut self, buffer: &mut [u8]) -> io::Result<Event> {
        if buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "demo needs a nonempty input buffer",
            ));
        }
        if self.pending.is_empty() {
            let at = self.started.elapsed();
            if at < self.next_at {
                // Keep cancellation responsive and never accumulate a catch-up queue.
                thread::sleep((self.next_at - at).min(Duration::from_millis(20)));
                return Ok(Event::Idle(State::Reading));
            }
            self.next_at = at + Duration::from_millis(10);
            self.pending = record_at(at, self.slot);
            self.slot = (self.slot + 1) % 10;
            if self.pending.is_empty() {
                return Ok(Event::Idle(State::Reading));
            }
        }
        let len = self.pending.len().min(buffer.len());
        buffer[..len].copy_from_slice(&self.pending[..len]);
        self.pending = &self.pending[len..];
        Ok(Event::Data { len, skip: 0 })
    }
}

fn record_at(at: Duration, slot: u8) -> &'static [u8] {
    let second = at.as_secs() % 40;
    match second {
        8..=15 if slot == 0 => b"level=warn msg=\"upstream latency rising (demo)\"\n",
        8..=15 if slot == 1 => b"{\"severity\":\"warning\",\"message\":\"upstream latency rising (demo)\"}\n",
        16..=23 if slot < 5 => [
            &b"ERROR upstream request timed out (demo)\n"[..],
            b"{\"severity\":\"error\",\"message\":\"upstream request timed out (demo)\"}\n",
            b"level=error msg=\"upstream request timed out (demo)\"\n",
            b"\x1b[31mERROR\x1b[0m upstream request timed out (demo)\n",
            b"<11>1 2026-11-05T12:00:00Z demo worker - - - upstream request timed out (demo)\n",
        ][slot as usize],
        16..=23 if slot < 7 => b"WARNING:worker:retrying upstream request (demo)\n",
        8..=23 if slot == 8 => b"{\"level\":50,\"msg\":\"demo: numeric convention needs --profile pino\"}\n",
        8..=23 if slot == 9 => b"{\"level\":\"info\",\"severity\":\"error\",\"message\":\"demo: choose --level-field severity\"}\n",
        24..=29 => b"",
        _ => [
            &b"INFO request completed (demo)\n"[..],
            b"{\"severity\":\"info\",\"message\":\"request completed (demo)\"}\n",
            b"level=info msg=\"ERROR is only a message word (demo)\"\n",
            b"INFO:worker:request completed (demo)\n",
            b"\x1b[32mINFO\x1b[0m request completed (demo)\n",
            b"<14>1 2026-11-05T12:00:00Z demo worker - - - ERROR is only a message word (demo)\n",
            b"{\"levelname\":\"INFO\",\"message\":\"request completed (demo)\"}\n",
            b"{\"log.level\":\"info\",\"message\":\"request completed (demo)\"}\n",
            b"2026-11-05 12:00:00,000 INFO request completed (demo)\n",
            b"{\"level\":\"info\",\"message\":\"request completed (demo)\"}\n",
        ][slot as usize],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::Args,
        engine::Engine,
        parse::{Classifier, Level, ParserKind},
    };

    use clap::Parser;

    #[test]
    fn cycle_has_predictable_severity_and_silence_without_external_input() {
        let classifier = Classifier::from_args(&Args::parse_from(["tell-tail", "--demo"]));
        for (second, records, warnings, errors, unknown) in [
            (0, 10, 0, 0, 0),
            (8, 10, 2, 0, 2),
            (16, 10, 2, 5, 2),
            (24, 0, 0, 0, 0),
            (30, 10, 0, 0, 0),
        ] {
            for cycle in [0, 40, 80] {
                let mut counts = [0; 7];
                for slot in 0..10 {
                    let line = record_at(Duration::from_secs(second + cycle), slot);
                    if !line.is_empty() {
                        let class = classifier.classify(line);
                        assert!(!class.parse_failure);
                        counts[class.level as usize] += 1;
                    }
                }
                assert_eq!(counts.iter().sum::<u32>(), records);
                assert_eq!(counts[Level::Warn as usize], warnings);
                assert_eq!(counts[Level::Error as usize], errors);
                assert_eq!(counts[Level::Unknown as usize], unknown);
            }
        }
    }

    #[test]
    fn mixed_demo_reports_real_parser_evidence_across_read_boundaries() {
        for chunk in [1, 7, 65536] {
            let mut engine = Engine::new(&Args::parse_from(["tell-tail", "--demo"]));
            for second in [0, 8, 16, 24, 30] {
                let at = Duration::from_secs(second);
                for slot in 0..10 {
                    for bytes in record_at(at, slot).chunks(chunk) {
                        engine.feed(bytes, at);
                    }
                }
            }
            let snapshot = engine.snapshot(Duration::from_secs(30), State::Reading, None);
            assert_eq!(snapshot.totals.records, 40);
            assert_eq!(snapshot.totals.parse_failures, 0);
            assert_eq!(snapshot.totals.levels.get(Level::Unknown), 4);
            let parsing = snapshot.parsing.unwrap();
            assert_eq!(parsing.numeric, 2);
            assert_eq!(parsing.conflicting, 2);
            for kind in [
                ParserKind::Text,
                ParserKind::Json,
                ParserKind::Logfmt,
                ParserKind::Syslog,
            ] {
                assert!(parsing.recognized[kind as usize] > 0);
            }
            assert_eq!(parsing.recognized.iter().sum::<u64>(), 36);
        }
    }

    #[test]
    fn small_reads_preserve_the_record_without_a_queue() {
        let mut demo = Demo::new();
        demo.pending = b"INFO example\n";
        let mut received = Vec::new();
        while !demo.pending.is_empty() {
            let mut buffer = [0; 3];
            let Event::Data { len, skip } = demo.next(&mut buffer).unwrap() else {
                panic!("pending data")
            };
            assert_eq!(skip, 0);
            received.extend_from_slice(&buffer[..len]);
        }
        assert_eq!(received, b"INFO example\n");
        assert!(demo.next(&mut []).is_err());
    }
}

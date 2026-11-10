//! A bounded, offline input for exploring the real ingestion and dashboard paths.
use super::{Event, State};
use crate::cli::{Args, Envelope, Profile};
use serde_json::{Map, Value, json};
use std::{
    io, thread,
    time::{Duration, Instant},
};

pub struct Demo {
    started: Instant,
    next_at: Duration,
    slot: u8,
    records: Vec<Vec<u8>>,
    pending: Option<(usize, usize)>,
}

impl Demo {
    pub fn new(args: &Args) -> Self {
        Self {
            started: Instant::now(),
            next_at: Duration::ZERO,
            slot: 0,
            // Four active phases, ten fixed examples each; never retain a growing queue.
            records: [0, 8, 16, 30]
                .into_iter()
                .flat_map(|second| (0..10).map(move |slot| example(args, second, slot)))
                .collect(),
            pending: None,
        }
    }

    pub fn next(&mut self, buffer: &mut [u8]) -> io::Result<Event> {
        if buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "demo needs a nonempty input buffer",
            ));
        }
        if self.pending.is_none() {
            let at = self.started.elapsed();
            if at < self.next_at {
                // Keep cancellation responsive and never accumulate a catch-up queue.
                thread::sleep((self.next_at - at).min(Duration::from_millis(20)));
                return Ok(Event::Idle(State::Reading));
            }
            self.next_at = at + Duration::from_millis(10);
            let slot = self.slot as usize;
            self.slot = (self.slot + 1) % 10;
            let phase = match at.as_secs() % 40 {
                0..=7 => 0,
                8..=15 => 1,
                16..=23 => 2,
                24..=29 => return Ok(Event::Idle(State::Reading)),
                _ => 3,
            };
            self.pending = Some((phase * 10 + slot, 0));
        }
        let (record, offset) = self.pending.unwrap();
        let remaining = &self.records[record][offset..];
        let len = remaining.len().min(buffer.len());
        buffer[..len].copy_from_slice(&remaining[..len]);
        self.pending = (len < remaining.len()).then_some((record, offset + len));
        Ok(Event::Data { len, skip: 0 })
    }
}

// Parser flags deliberately select matching synthetic input. The ingestion engine
// still uses the real classifier; no classification or diagnostics are fabricated.
fn example(args: &Args, second: u64, slot: u8) -> Vec<u8> {
    let mut record =
        if args.profile.is_some() || args.level_field.is_some() || args.level_path.is_some() {
            let severity = match second {
                8 if slot < 2 => 1,
                16 if slot < 5 => 2,
                16 if slot < 7 => 1,
                _ => 0,
            };
            let level = match args.profile {
                Some(Profile::Pino) => json!([30, 40, 50][severity]),
                Some(Profile::Python) => json!([20, 30, 40][severity]),
                Some(Profile::Otel) => json!([9, 13, 17][severity]),
                Some(Profile::Serilog) => json!(["Information", "Warning", "Error"][severity]),
                None => json!(["info", "warn", "error"][severity]),
            };
            let keys = args
                .level_path
                .as_deref()
                .map(|path| crate::parse::pointer_tokens(path).expect("validated demo path"))
                .unwrap_or_else(|| {
                    vec![
                        args.level_field
                            .as_deref()
                            .or_else(|| args.profile.map(Profile::field))
                            .unwrap_or("level")
                            .to_owned(),
                    ]
                });
            let mut document = level;
            for key in keys.into_iter().rev() {
                let mut object = Map::new();
                object.insert(key, document);
                document = Value::Object(object);
            }
            // A custom severity key may itself be named message; do not overwrite it.
            document
                .as_object_mut()
                .unwrap()
                .entry("message")
                .or_insert_with(|| {
                    json!(
                        [
                            "request completed (demo)",
                            "retrying upstream request (demo)",
                            "upstream request timed out (demo)",
                        ][severity]
                    )
                });
            let mut bytes = serde_json::to_vec(&document).expect("synthetic JSON");
            bytes.push(b'\n');
            bytes
        } else {
            record_at(Duration::from_secs(second), slot).to_vec()
        };
    if let Some(envelope) = args.envelope {
        let text = std::str::from_utf8(&record).expect("synthetic UTF-8");
        record = match envelope {
            Envelope::Docker => serde_json::to_vec(&json!({
                "log": text, "stream": "stdout", "time": "2026-11-05T12:00:00Z",
            }))
            .expect("synthetic Docker JSON"),
            Envelope::Cri => format!(
                "2026-11-05T12:00:00Z stdout F {}",
                text.trim_end_matches('\n')
            )
            .into_bytes(),
        };
        record.push(b'\n');
    }
    record
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
    fn selectors_generate_examples_that_the_real_engine_recognizes() {
        for flags in [
            vec!["--profile", "pino"],
            vec!["--profile", "python"],
            vec!["--profile", "otel"],
            vec!["--profile", "serilog"],
            vec!["--level-field", "priority"],
            vec!["--level-field", "message"],
            vec!["--level-field", "log.level"],
            vec!["--level-path", "/log/level"],
            vec!["--level-path", "/message/a~1b/~0level"],
            vec!["--level-path", "/"],
            vec!["--profile", "python", "--level-path", "/event/priority"],
            vec!["--profile", "serilog", "--level-field", "priority"],
        ] {
            for envelope in [
                vec![],
                vec!["--envelope", "docker"],
                vec!["--envelope", "cri"],
            ] {
                let args = Args::parse_from(
                    [vec!["tell-tail", "--demo"], flags.clone(), envelope].concat(),
                );
                args.validate().unwrap();
                let demo = Demo::new(&args);
                assert_eq!(demo.records.len(), 40);
                for chunk in [1, 7, 65536] {
                    let mut engine = Engine::new(&args);
                    for line in &demo.records {
                        for bytes in line.chunks(chunk) {
                            engine.feed(bytes, Duration::ZERO);
                        }
                    }
                    let snapshot = engine.snapshot(Duration::from_secs(1), State::Reading, None);
                    assert_eq!(snapshot.totals.records, 40);
                    assert_eq!(snapshot.totals.parse_failures, 0, "{args:?}");
                    for (level, count) in [
                        (Level::Info, 31),
                        (Level::Warn, 4),
                        (Level::Error, 5),
                        (Level::Unknown, 0),
                    ] {
                        assert_eq!(snapshot.totals.levels.get(level), count, "{args:?}");
                    }
                    if args.profile.is_some() {
                        assert_eq!(
                            snapshot.parsing.unwrap().recognized[ParserKind::Profile as usize],
                            40
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn envelopes_preserve_the_mixed_demo_and_its_hints() {
        for envelope in ["docker", "cri"] {
            let args = Args::parse_from(["tell-tail", "--demo", "--envelope", envelope]);
            args.validate().unwrap();
            let demo = Demo::new(&args);
            let mut engine = Engine::new(&args);
            for line in &demo.records {
                engine.feed(line, Duration::ZERO);
            }
            let snapshot = engine.snapshot(Duration::ZERO, State::Reading, None);
            assert_eq!(snapshot.totals.records, 40);
            assert_eq!(snapshot.totals.parse_failures, 0);
            assert_eq!(snapshot.totals.levels.get(Level::Unknown), 4);
            assert_eq!(snapshot.parsing.unwrap().numeric, 2);
            assert_eq!(snapshot.parsing.unwrap().conflicting, 2);
        }
    }

    #[test]
    fn small_reads_preserve_the_record_without_a_queue() {
        let mut demo = Demo::new(&Args::parse_from(["tell-tail", "--demo"]));
        let expected = demo.records[0].clone();
        demo.pending = Some((0, 0));
        let mut received = Vec::new();
        while demo.pending.is_some() {
            let mut buffer = [0; 3];
            let Event::Data { len, skip } = demo.next(&mut buffer).unwrap() else {
                panic!("pending data")
            };
            assert_eq!(skip, 0);
            received.extend_from_slice(&buffer[..len]);
        }
        assert_eq!(received, expected);
        assert!(demo.next(&mut []).is_err());
    }
}

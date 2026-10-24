use clap::Parser;
use proptest::prelude::*;
use std::time::Duration;
use tell_tail::{
    cli::Args,
    engine::Engine,
    framing::Framer,
    parse::{Classification, Level},
    stats::{State, Stats},
};

fn reference_lines(input: &[u8], limit: usize) -> Vec<(Vec<u8>, bool)> {
    let mut lines: Vec<_> = input.split(|b| *b == b'\n').collect();
    if lines.last() == Some(&&b""[..]) {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| {
            if line.len() > limit {
                (vec![], true)
            } else {
                (line.strip_suffix(b"\r").unwrap_or(line).to_vec(), false)
            }
        })
        .collect()
}
proptest! {
    #![proptest_config(ProptestConfig { cases: 128, max_shrink_iters: 2048, ..ProptestConfig::default() })]
    #[test]
    fn arbitrary_bytes_match_reference_framing(
        bytes in prop::collection::vec(any::<u8>(), 0..4096),
        chunks in prop::collection::vec(1usize..257, 1..32),
        limit in 1usize..2049,
    ) {
        let expected = reference_lines(&bytes, limit);
        let mut framer = Framer::new(limit);
        let mut actual = vec![];
        let mut position = 0;
        for chunk in chunks.iter().cycle() {
            if position == bytes.len() { break; }
            let end = (position + chunk).min(bytes.len());
            framer.feed(&bytes[position..end], |line, oversized| actual.push((line.to_vec(), oversized)));
            position = end;
        }
        framer.finish(|line, oversized| actual.push((line.to_vec(), oversized)));
        framer.finish(|_, _| panic!("EOF emitted twice"));
        prop_assert_eq!(framer.pending_bytes(), 0);
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn mixed_generated_records_match_known_counts(
        records in prop::collection::vec((0usize..9, 0usize..256, any::<bool>()), 0..64),
        chunk in 1usize..1025, limit in 1u32..513, terminated in any::<bool>(),
    ) {
        let mut args = Args::parse_from(["tell-tail"]);
        args.max_record_bytes = limit;
        let mut input = Vec::new();
        let mut counts = [0u64; 7];
        let mut failures = 0;
        let mut oversized = 0;
        for &(kind, length, json) in &records {
            let level = Level::ALL[kind.min(6)];
            let line = match kind {
                7 => "{bad}".into(),
                8 => String::new(),
                _ if json => serde_json::json!({"level":level.name(), "message":"x".repeat(length)}).to_string(),
                _ => format!("{} {}", level.name(), "x".repeat(length)),
            };
            let too_large = line.len() > limit as usize;
            let expected = if too_large || kind >= 7 { Level::Unknown } else { level };
            counts[expected as usize] += 1;
            oversized += u64::from(too_large);
            failures += u64::from(kind == 7 && !too_large);
            input.extend_from_slice(line.as_bytes());
            input.push(b'\n');
        }
        if !terminated && input.last() == Some(&b'\n') {
            input.pop();
            // An empty final segment without a delimiter is not a physical record.
            if records.last().is_some_and(|r| r.0 == 8) { counts[6] -= 1; }
        }
        let mut engine = Engine::new(&args);
        for bytes in input.chunks(chunk) { engine.feed(bytes, Duration::ZERO); }
        engine.finish(Duration::ZERO);
        engine.finish(Duration::ZERO);
        let report = engine.snapshot(Duration::from_secs(1), State::Complete, None);
        prop_assert_eq!(report.totals.levels.0, counts);
        prop_assert_eq!(report.totals.records, counts.iter().sum::<u64>());
        prop_assert_eq!(report.totals.parse_failures, failures);
        prop_assert_eq!(report.totals.oversized_records, oversized);
        prop_assert_eq!(report.bytes_read, input.len() as u64);
    }

    #[test]
    fn timed_operations_match_an_independent_event_ledger(
        window in prop_oneof![Just(1u32), Just(60), Just(3600)],
        operations in prop::collection::vec((0u64..10_000_000, 0usize..7, any::<bool>()), 0..32),
    ) {
        let mut stats = Stats::new(window, "generated".into());
        let mut ledger = vec![];
        let mut millis = 0;
        for (advance, index, record) in operations {
            millis += advance;
            let at = Duration::from_millis(millis);
            if record {
                let level = Level::ALL[index];
                stats.record(Classification {level, parse_failure: false}, false, at);
                ledger.push((millis, index));
            }
            let report = stats.snapshot(at, State::Following, 0, None);
            let left = millis.saturating_sub(u64::from(window) * 1000) / 100 * 100;
            let mut expected = [0u64; 7];
            for &(time, level) in &ledger {
                if time >= left { expected[level] += 1; }
            }
            prop_assert_eq!(report.window.counts.levels.0, expected);
            prop_assert_eq!(report.window.counts.records, expected.iter().sum::<u64>());
            prop_assert_eq!(report.totals.records, ledger.len() as u64);
            prop_assert_eq!(report.totals.levels.total(), report.totals.records);
            // Duration conversion and division of integer milliseconds can
            // round to adjacent floats (for example, at 3119 ms).
            let expected_seconds = (millis - left) as f64 / 1000.0;
            prop_assert!((report.window.observed_seconds - expected_seconds).abs() < 1e-9);
        }
    }
}

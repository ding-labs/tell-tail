#![no_main]
use clap::Parser;
use libfuzzer_sys::fuzz_target;
use std::time::Duration;
use tell_tail::{cli::Args, engine::Engine, parse::Level, stats::State};

fuzz_target!(|data: &[u8]| {
    let mut data = data;
    if data.len() < 3 {
        return;
    }
    let mut args = Args::parse_from(["tell-tail"]);
    args.window = u32::from(data[0] % 60) + 1;
    args.max_record_bytes = u32::from(u16::from_le_bytes([data[1], data[2]])) + 1;
    data = &data[3..];
    let mut engine = Engine::new(&args);
    let mut millis = 0;
    let mut consumed = 0;
    let mut previous = 0;
    while data.len() >= 2 {
        let operation = data[0] % 4;
        let parameter = usize::from(data[1]);
        data = &data[2..];
        match operation {
            0 => {
                let len = parameter.min(data.len());
                engine.feed(&data[..len], Duration::from_millis(millis));
                consumed += len as u64;
                data = &data[len..];
            }
            1 => millis += (parameter as u64) * 1000,
            2 => engine.finish(Duration::from_millis(millis)),
            _ => engine.discard_partial(),
        }
        let s = engine.snapshot(Duration::from_millis(millis), State::Reading, None);
        assert_eq!(s.bytes_read, consumed);
        assert_eq!(s.totals.records, s.totals.levels.total());
        assert!(s.totals.records >= previous);
        assert!(s.totals.parse_failures <= s.totals.levels.get(Level::Unknown));
        assert!(s.totals.oversized_records <= s.totals.levels.get(Level::Unknown));
        assert!(s.pending_bytes <= consumed && s.skipped_bytes <= consumed);
        previous = s.totals.records;
    }
});

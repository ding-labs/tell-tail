//! Finite synthetic replay: never discovers or reads desktop logs.
use super::{
    model::{MAX_RECORD_BYTES, Observation, SCHEMA_VERSION},
    store::Store,
};
use crate::{framing::Framer, mcp::cli::Output, runtime::Signals};
use std::{
    io::{self, Read, Write},
    path::Path,
    sync::atomic::Ordering,
};

fn envelope(output: &mut impl Write, mode: Output, value: serde_json::Value) -> io::Result<()> {
    if mode == Output::Json {
        writeln!(output, "{value}")
    } else if let Some(call) = value.get("call") {
        writeln!(
            output,
            "{} / {} · {} · {}",
            call["plugin"].as_str().unwrap_or("unidentified plugin"),
            call["tool"].as_str().unwrap_or("unknown tool"),
            call["status"].as_str().unwrap_or("unknown"),
            if call["historical"] == true {
                "imported replay"
            } else {
                "observed result"
            }
        )
    } else {
        writeln!(
            output,
            "Trace replay · {}",
            value["kind"].as_str().unwrap_or("status")
        )
    }
}

pub(crate) fn updates(
    store: &Store,
    sequence: &mut u64,
    output: &mut impl Write,
    mode: Output,
) -> io::Result<()> {
    let snapshot = store.snapshot();
    if *sequence < snapshot.oldest_watermark {
        envelope(
            output,
            mode,
            serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"trace_gap",
            "reason":"retention_eviction","oldest_watermark":snapshot.oldest_watermark.to_string()}),
        )?;
    }
    for call in store.updates_after(*sequence) {
        envelope(
            output,
            mode,
            serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"call_update","call":call}),
        )?;
    }
    *sequence = store.snapshot().watermark;
    Ok(())
}

pub fn run(path: &Path, mode: Output, signals: &Signals) -> io::Result<i32> {
    let file = crate::platform::open_regular(path)?;
    // Freeze the input boundary so a growing file cannot make this replay endless.
    let size = file.metadata()?.len();
    let mut input = file.take(size);
    let mut output = crate::platform::StdoutWriter::new(signals.requested.clone())?;
    replay(&mut input, &mut output, mode, || {
        signals.requested.load(Ordering::Relaxed) != 0
    })?;
    output.flush()?;
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}

fn replay(
    input: &mut impl Read,
    output: &mut impl Write,
    mode: Output,
    cancelled: impl Fn() -> bool,
) -> io::Result<()> {
    let mut store = Store::default();
    let mut framer = Framer::new(MAX_RECORD_BYTES);
    let mut buffer = [0; 16 * 1024];
    let mut sequence = 0;
    let mut invalid = 0u64;
    let mut oversized = 0u64;
    let mut emit = |bytes: &[u8], large: bool| -> io::Result<()> {
        let parsed = if large {
            oversized += 1;
            Err("Oversized trace record")
        } else {
            Observation::parse(bytes)
        };
        match parsed.and_then(|o| store.accept(o, crate::mcp::time::now_ms(), true)) {
            Ok(_) => (),
            Err(_) => {
                invalid += 1;
                store.mark_incomplete().map_err(io::Error::other)?;
                envelope(
                    output,
                    mode,
                    serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"trace_gap","reason":"invalid_replay_record"}),
                )?;
            }
        }
        updates(&store, &mut sequence, output, mode)
    };
    while !cancelled() {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let mut error = None;
        framer.feed(&buffer[..n], |bytes, large| {
            if error.is_none() {
                error = emit(bytes, large).err();
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
    }
    if !cancelled() {
        let mut error = None;
        framer.finish(|bytes, large| {
            error = emit(bytes, large).err();
        });
        if let Some(error) = error {
            return Err(error);
        }
    }
    store.mark_incomplete().map_err(io::Error::other)?;
    updates(&store, &mut sequence, output, mode)?;
    envelope(
        output,
        mode,
        serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"trace_source",
        "mode":"synthetic_replay","live_capture_verified":false,"running":false,
        "invalid_records":invalid.to_string(),"oversized_records":oversized.to_string(),"snapshot":store.snapshot()}),
    )
}

/// Populate a web replay without discovering logs or opening an intake listener.
#[cfg(feature = "web")]
pub fn load_web(
    file: std::fs::File,
    shared: &super::live::Shared,
    stop: &std::sync::atomic::AtomicUsize,
) -> io::Result<()> {
    let size = file.metadata()?.len();
    let mut input = file.take(size);
    let mut framer = Framer::new(MAX_RECORD_BYTES);
    let mut buffer = [0; 16 * 1024];
    let mut emit = |bytes: &[u8], large: bool| {
        if let Ok(mut state) = shared.lock() {
            let observation = if large {
                Err("Oversized replay record")
            } else {
                Observation::parse(bytes)
            };
            match observation.and_then(|o| state.store.accept(o, crate::mcp::time::now_ms(), true))
            {
                Ok(_) => (),
                Err(_) => {
                    state.rejected_records += 1;
                    let _ = state.store.mark_incomplete();
                }
            }
        }
    };
    while stop.load(Ordering::Relaxed) == 0 {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        framer.feed(&buffer[..n], &mut emit);
    }
    if stop.load(Ordering::Relaxed) == 0 {
        framer.finish(&mut emit);
    }
    if let Ok(mut state) = shared.lock() {
        state.store.mark_incomplete().map_err(io::Error::other)?;
        state.running = false;
    }
    Ok(())
}

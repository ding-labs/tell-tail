//! One follower feeds both domains. Trace projection pulls bounded accepted updates
//! without holding domain and activity locks together; coalescing/loss is disclosed.
use super::{
    parse::{self, Parsed},
    project,
    store::{Sources, Store, TraceSource},
};
use crate::mcp::{
    cli::{Args, Output},
    collection::{Collector, Record},
    runtime::Running,
};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

pub type Shared = Arc<Mutex<Store>>;
pub struct Bridge {
    pub shared: Shared,
    trace_sequence: u64,
    unsupported: u64,
    ignored: u64,
    gaps: u64,
    source_tick: Instant,
}
impl Bridge {
    pub fn new(shared: Shared) -> Self {
        Self {
            shared,
            trace_sequence: 0,
            unsupported: 0,
            ignored: 0,
            gaps: 0,
            source_tick: Instant::now() - Duration::from_secs(1),
        }
    }
    fn accept(&mut self, key: String, item: super::model::Item) {
        if self
            .shared
            .lock()
            .map_or(true, |mut s| s.accept(key, item).is_err())
        {
            self.gaps += 1;
        }
    }
    pub fn record(&mut self, record: Record<'_>, mcp: &crate::mcp::runtime::Shared) {
        let parsed = parse::record(&record, crate::mcp::time::now_ms());
        let key = format!(
            "file-{}-{}-{}",
            record.source_id, record.generation, record.offset
        );
        let event = mcp.lock().ok().and_then(|mut s| s.accept_delta(record));
        if let Some(event) = event {
            self.accept(format!("mcp-{}", event.id), project::mcp(&event));
        } else {
            match parsed {
                Parsed::Event(item) => self.accept(key, *item),
                Parsed::Unsupported => self.unsupported += 1,
                Parsed::Ignored => self.ignored += 1,
            }
        }
    }
    pub fn sync(
        &mut self,
        collector: Option<&Collector>,
        traces: &crate::tool_trace::live::Shared,
        running: bool,
        demo: bool,
    ) {
        let (updates, watermark, trace) = match traces.lock() {
            Ok(s) => (
                s.store.updates_after(self.trace_sequence),
                s.store.snapshot().watermark,
                TraceSource::from_state(&s),
            ),
            Err(_) => (
                Vec::new(),
                self.trace_sequence,
                TraceSource {
                    status: "unavailable",
                    ..TraceSource::default()
                },
            ),
        };
        // The domain store exposes current revisions. Intermediate revisions can
        // coalesce, or old calls can be evicted before projection; never hide that.
        self.gaps += watermark
            .saturating_sub(self.trace_sequence)
            .saturating_sub(updates.len() as u64);
        for call in updates {
            self.accept(format!("call-{}", call.id), project::call(&call));
        }
        self.trace_sequence = watermark;
        if running && self.source_tick.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.source_tick = Instant::now();
        let sources = Sources {
            running,
            demo,
            roots: collector.map(|c| c.root_views.clone()).unwrap_or_default(),
            files: collector
                .map(Collector::views)
                .unwrap_or_default()
                .into_iter()
                .map(super::store::sources::LogFile)
                .collect(),
            discovery_limited: collector.is_some_and(|c| c.limited),
            rejected_files: collector.map_or(0, |c| c.rejected as u64),
            open_failures: collector.map_or(0, |c| c.open_failures),
            retired_sources: collector.map_or(0, |c| c.retired),
            unsupported_records: self.unsupported,
            ignored_records: self.ignored,
            projection_gaps: self.gaps,
            trace,
        };
        if self
            .shared
            .lock()
            .map_or(true, |mut s| s.update_sources(sources).is_err())
        {
            self.gaps += 1;
        }
    }
}

pub fn run(args: &Args, signals: &crate::runtime::Signals) -> io::Result<i32> {
    let mut running = Running::start(args, signals.requested.clone())?;
    let shared = running
        .activity
        .as_ref()
        .expect("desktop activity enabled")
        .clone();
    let mut output = crate::platform::StdoutWriter::new(signals.requested.clone())?;
    let mut sequence = 0;
    let mut status_at = Instant::now() - Duration::from_secs(1);
    loop {
        if args.once && !running.is_finished() {
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        let (snapshot, updates) = {
            let s = shared
                .lock()
                .map_err(|_| io::Error::other("Activity store unavailable"))?;
            (
                s.snapshot(),
                s.view(s.sequence)
                    .map_err(io::Error::other)?
                    .into_iter()
                    .filter(|i| i.revision > sequence)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        if snapshot.oldest_watermark > sequence {
            writeln!(
                output,
                "{}",
                serde_json::json!({"schema_version":1,"kind":"activity_gap","after":sequence.to_string(),"oldest_available":snapshot.oldest_watermark.to_string()})
            )?;
        }
        let mut updates = updates;
        updates.sort_by_key(|i| i.revision);
        for item in updates {
            match args.mode() {
                Output::Json => writeln!(
                    output,
                    "{}",
                    serde_json::json!({"schema_version":1,"kind":"activity_item","item":item})
                )?,
                _ => writeln!(
                    output,
                    "{} · {} · {}{}",
                    item.category.key(),
                    item.subject,
                    item.summary,
                    if item.historical { " [history]" } else { "" }
                )?,
            }
        }
        sequence = snapshot.watermark;
        if status_at.elapsed() >= Duration::from_secs(1) || running.is_finished() {
            match args.mode() {
                Output::Json => writeln!(
                    output,
                    "{}",
                    serde_json::json!({"schema_version":1,"kind":"activity_sources","snapshot":snapshot})
                )?,
                _ => writeln!(
                    output,
                    "Desktop activity: {} log sources · {} retained items · tool telemetry {}",
                    snapshot.sources.files.len(),
                    snapshot.retained_items,
                    snapshot.sources.trace.status
                )?,
            }
            status_at = Instant::now();
        }
        output.flush()?;
        if running.is_finished() {
            if snapshot.sources.running {
                continue;
            }
            running.join()?;
            break;
        }
        if signals.requested.load(Ordering::Relaxed) != 0 {
            running.join()?;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}

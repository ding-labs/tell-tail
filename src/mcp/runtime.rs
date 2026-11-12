use super::{
    cli::{Args, Output},
    collection::Collector,
    demo::Demo,
    model::SCHEMA_VERSION,
    store::{Snapshot, Store},
};
use crate::runtime::Signals;
use std::{
    io::{self, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub type Shared = Arc<Mutex<Store>>;
pub struct Running {
    pub shared: Shared,
    stop: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}
impl Running {
    pub fn start(args: &Args, stop: Arc<AtomicUsize>) -> io::Result<Self> {
        let mut demo = args.demo.then(Demo::new).transpose()?;
        let roots = match &demo {
            Some(d) => d.roots(),
            None => args.roots()?,
        };
        let once = args.once;
        let shared = Arc::new(Mutex::new(Store::default()));
        shared.lock().unwrap().demo = args.demo;
        let slot = shared.clone();
        let cancel = stop.clone();
        let worker = thread::Builder::new()
            .name("tell-tail-mcp".into())
            .spawn(move || {
                let mut collector = Collector::new(roots);
                loop {
                    if let Some(demo) = &mut demo {
                        demo.tick().expect("synthetic replay failed");
                    }
                    collector.poll(|r| slot.lock().unwrap().accept(r));
                    let backlog = collector.views().iter().any(|s| s.backlog_bytes > 0);
                    {
                        let mut s = slot.lock().unwrap();
                        let sources = collector.views();
                        if s.sources != sources
                            || s.roots != collector.root_views
                            || s.discovery_limited != collector.limited
                            || s.rejected_files != collector.rejected
                            || s.open_failures != collector.open_failures
                            || s.retired_sources != collector.retired
                        {
                            s.sequence += 1;
                        }
                        s.sources = sources;
                        s.roots = collector.root_views.clone();
                        s.discovery_limited = collector.limited;
                        s.rejected_files = collector.rejected;
                        s.open_failures = collector.open_failures;
                        s.retired_sources = collector.retired;
                    }
                    if cancel.load(Ordering::Relaxed) != 0 || (once && !backlog) {
                        break;
                    }
                    thread::sleep(if backlog {
                        Duration::from_millis(1)
                    } else {
                        Duration::from_millis(100)
                    });
                }
                let mut s = slot.lock().unwrap();
                s.running = false;
                s.sequence += 1;
            })?;
        Ok(Self {
            shared,
            stop,
            worker: Some(worker),
        })
    }
    pub fn snapshot(&self) -> io::Result<Snapshot> {
        self.shared
            .lock()
            .map(|s| s.snapshot())
            .map_err(|_| io::Error::other("MCP collector failed"))
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(|w| w.is_finished())
    }
    pub fn stop(&self) {
        let _ = self
            .stop
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed);
    }
    pub fn join(&mut self) -> io::Result<()> {
        self.worker.take().map_or(Ok(()), |w| {
            w.join()
                .map_err(|_| io::Error::other("MCP collector panicked"))
        })
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
        let _ = self.join();
    }
}

pub fn run(args: &Args) -> io::Result<i32> {
    let signals = Signals::install()?;
    #[cfg(feature = "web")]
    if args.mode() == Output::Web {
        return crate::web::mcp::run(args, &signals);
    }
    let mut running = Running::start(args, signals.requested.clone())?;
    let mut output = crate::platform::StdoutWriter::new(signals.requested.clone())?;
    let mut last_id = 0;
    let mut status_at = Instant::now() - Duration::from_secs(1);
    loop {
        let snap = running.snapshot()?;
        if args.once && snap.running && !running.is_finished() {
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        let (events, oldest) = {
            let s = running
                .shared
                .lock()
                .map_err(|_| io::Error::other("MCP collector failed"))?;
            (
                s.events
                    .iter()
                    .filter(|e| e.id > last_id)
                    .take(100)
                    .cloned()
                    .collect::<Vec<_>>(),
                s.events.front().map(|e| e.id),
            )
        };
        if oldest.is_some_and(|id| id > last_id + 1) {
            writeln!(
                output,
                "{}",
                serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"gap","after":last_id,"oldest_available":oldest})
            )?;
        }
        for event in events {
            last_id = event.id;
            match args.mode() {
                Output::Json => writeln!(
                    output,
                    "{}",
                    serde_json::json!({"schema_version":SCHEMA_VERSION,"kind":"event","event":event})
                )?,
                _ => writeln!(
                    output,
                    "{} · {} · {}{}",
                    event.host.label(),
                    event.server,
                    event.summary,
                    if event.historical { " [history]" } else { "" }
                )?,
            }
        }
        if status_at.elapsed() >= Duration::from_secs(1) || !snap.running {
            match args.mode() {
                Output::Json => {
                    let mut value = serde_json::to_value(&snap)?;
                    value["kind"] = "sources".into();
                    value.as_object_mut().unwrap().remove("events");
                    writeln!(output, "{value}")?;
                }
                _ => writeln!(
                    output,
                    "MCP watcher: {} source(s), {} observations, {} mirrored, {} retained issue(s); {}",
                    snap.sources.len(),
                    snap.counts.observations,
                    snap.counts.mirrors,
                    snap.retained_issues,
                    if snap.sources.is_empty() {
                        "waiting for supported logs (use --source and --log-dir for an explicit directory)"
                    } else {
                        "last observations are not health checks"
                    }
                )?,
            }
            status_at = Instant::now();
        }
        output.flush()?;
        if running.is_finished() {
            running.join()?;
            let latest = running.snapshot()?;
            if !snap.running && latest.events.first().is_none_or(|e| e.id <= last_id) {
                break;
            }
        }
        if signals.requested.load(Ordering::Relaxed) != 0 {
            running.stop();
            running.join()?;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let signal = signals.requested.load(Ordering::Relaxed);
    Ok(if signal > 1 { 128 + signal as i32 } else { 0 })
}

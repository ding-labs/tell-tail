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
    pub activity: Option<crate::activity::runtime::Shared>,
    pub logs: Option<super::log_metrics::Shared>,
    pub traces: crate::tool_trace::live::Shared,
    #[cfg(feature = "web")]
    trace_receiver: Option<crate::tool_trace::receiver::Running>,
    stop: Arc<AtomicUsize>,
    worker: Option<JoinHandle<()>>,
}
impl Running {
    pub fn start(args: &Args, stop: Arc<AtomicUsize>) -> io::Result<Self> {
        let logs = (args.mode() == Output::Web).then(|| {
            Arc::new(Mutex::new(super::log_metrics::LogMetrics::new(
                args.demo || args.trace_replay.is_some(),
            )))
        });
        let activity = args
            .desktop
            .then(|| Arc::new(Mutex::new(crate::activity::store::Store::default())));
        let traces = Arc::new(Mutex::new(crate::tool_trace::live::State::default()));
        #[cfg(feature = "web")]
        let trace_receiver = args
            .trace_port
            .map(|port| {
                crate::tool_trace::receiver::Running::start(
                    port,
                    args.trace_token_file
                        .as_deref()
                        .expect("clap requires credential file"),
                    args.trace_config_out.as_deref(),
                    args.inspect_chat_index.as_deref(),
                    traces.clone(),
                    stop.clone(),
                )
            })
            .transpose()?;
        if let Some(path) = &args.trace_replay {
            if let Some(logs) = &logs {
                logs.lock().unwrap().update(&[], false);
            }
            let file = crate::platform::open_regular(path)?;
            let shared = Arc::new(Mutex::new(Store::default()));
            shared.lock().unwrap().demo = true;
            {
                let mut t = traces.lock().unwrap();
                t.enabled = true;
                t.replay = true;
                t.running = true;
            }
            let trace_slot = traces.clone();
            let slot = shared.clone();
            let cancel = stop.clone();
            let projection = activity.clone();
            let once = args.once;
            let worker = thread::Builder::new()
                .name("tell-tail-trace-replay".into())
                .spawn(move || {
                    let mut bridge = projection.map(crate::activity::runtime::Bridge::new);
                    if crate::tool_trace::replay::load_web(file, &trace_slot, &cancel).is_err() {
                        let mut t = trace_slot.lock().unwrap();
                        t.rejected_records += 1;
                        t.running = false;
                    }
                    while cancel.load(Ordering::Relaxed) == 0 {
                        if let Some(b) = &mut bridge {
                            b.sync(None, &trace_slot, !once, true);
                        }
                        if once {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                    if let Some(b) = &mut bridge {
                        b.sync(None, &trace_slot, false, true);
                    }
                    let mut s = slot.lock().unwrap();
                    s.running = false;
                    s.sequence += 1;
                })?;
            return Ok(Self {
                shared,
                logs,
                activity,
                traces,
                #[cfg(feature = "web")]
                trace_receiver,
                stop,
                worker: Some(worker),
            });
        }
        let mut demo = args.demo.then(Demo::new).transpose()?;
        if args.desktop
            && let Some(demo) = &mut demo
        {
            demo.enable_desktop()?;
            crate::activity::demo::seed(&traces);
        }
        let roots = match &demo {
            Some(d) => d.roots(),
            None => args.roots()?,
        };
        let once = args.once;
        let shared = Arc::new(Mutex::new(Store::default()));
        shared.lock().unwrap().demo = args.demo;
        let slot = shared.clone();
        let projection = activity.clone();
        let trace_slot = traces.clone();
        let log_slot = logs.clone();
        let demo_mode = args.demo;
        let cancel = stop.clone();
        let worker = thread::Builder::new()
            .name("tell-tail-mcp".into())
            .spawn(move || {
                let mut bridge = projection.map(crate::activity::runtime::Bridge::new);
                let mut collector = Collector::new(roots);
                loop {
                    if let Some(demo) = &mut demo {
                        demo.tick().expect("synthetic replay failed");
                    }
                    collector.poll(|r| {
                        if let Some(logs) = &log_slot {
                            logs.lock().unwrap().record(r.bytes);
                        }
                        if let Some(b) = &mut bridge {
                            b.record(r, &slot);
                        } else {
                            slot.lock().unwrap().accept(r);
                        }
                    });
                    if let Some(b) = &mut bridge {
                        b.sync(Some(&collector), &trace_slot, true, demo_mode);
                    }
                    if let Some(logs) = &log_slot {
                        logs.lock().unwrap().update(&collector.views(), true);
                    }
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
                if let Some(logs) = &log_slot {
                    logs.lock().unwrap().update(&collector.views(), false);
                }
                let mut s = slot.lock().unwrap();
                s.running = false;
                s.sequence += 1;
                drop(s);
                if let Some(b) = &mut bridge {
                    b.sync(Some(&collector), &trace_slot, false, demo_mode);
                }
            })?;
        Ok(Self {
            shared,
            logs,
            activity,
            traces,
            #[cfg(feature = "web")]
            trace_receiver,
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
        self.stop();
        #[cfg(feature = "web")]
        if let Some(receiver) = &mut self.trace_receiver {
            receiver.join()?;
        }
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
    if args.desktop {
        return crate::activity::runtime::run(args, &signals);
    }
    if let Some(path) = &args.trace_replay {
        return crate::tool_trace::replay::run(path, args.mode(), &signals);
    }
    let mut running = Running::start(args, signals.requested.clone())?;
    let mut output = crate::platform::StdoutWriter::new(signals.requested.clone())?;
    let mut last_id = 0;
    let mut trace_sequence = 0;
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
        if args.trace_port.is_some() {
            let traces = running
                .traces
                .lock()
                .map_err(|_| io::Error::other("Trace collector failed"))?;
            crate::tool_trace::replay::updates(
                &traces.store,
                &mut trace_sequence,
                &mut output,
                args.mode(),
            )?;
            if status_at.elapsed() >= Duration::from_secs(1) || !snap.running {
                if args.mode() == Output::Json {
                    writeln!(output, "{}", traces.snapshot())?;
                } else {
                    writeln!(
                        output,
                        "Tool tracing: {} · result-only · {} retained calls",
                        traces.snapshot()["status"].as_str().unwrap_or("unknown"),
                        traces.store.snapshot().retained_calls
                    )?;
                }
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

use crate::{
    cli::Args,
    engine::Engine,
    input::{Event, Input},
    stats::{Snapshot, State},
};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Signals {
    pub requested: Arc<AtomicUsize>,
    registrations: Vec<signal_hook::SigId>,
}
impl Signals {
    pub fn install() -> io::Result<Self> {
        let requested = Arc::new(AtomicUsize::new(0));
        let mut guard = Self {
            requested,
            registrations: vec![],
        };
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            guard.registrations.push(signal_hook::flag::register_usize(
                signal,
                guard.requested.clone(),
                signal as usize,
            )?);
        }
        Ok(guard)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in &self.registrations {
            signal_hook::low_level::unregister(*id);
        }
    }
}

pub struct Running {
    latest: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicUsize>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Running {
    pub fn start(args: &Args, stop: Arc<AtomicUsize>) -> io::Result<Self> {
        let input = Input::open(args)?;
        let engine = Engine::new(args);
        let latest = Arc::new(Mutex::new(engine.snapshot(
            Duration::ZERO,
            State::Reading,
            input.backlog(),
        )));
        let slot = latest.clone();
        let cancel = stop.clone();
        let worker = thread::Builder::new()
            .name("tell-tail-reader".into())
            .spawn(move || drive(input, engine, slot, cancel))?;
        Ok(Self {
            latest,
            stop,
            worker: Some(worker),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        self.latest
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
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
        match self.worker.take() {
            Some(w) => w
                .join()
                .map_err(|_| io::Error::other("ingestion worker panicked"))?,
            None => Ok(()),
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
        let _ = self.join();
    }
}
fn publish(slot: &Mutex<Snapshot>, snapshot: Snapshot) {
    // The reader only contends with a small snapshot clone, never output I/O.
    *slot.lock().unwrap_or_else(|p| p.into_inner()) = snapshot;
}
fn drive(
    mut input: Input,
    mut engine: Engine,
    slot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicUsize>,
) -> io::Result<()> {
    let started = Instant::now();
    let mut published = Instant::now();
    let mut buffer = vec![0; 64 * 1024];
    let mut state = State::Reading;
    let result = (|| -> io::Result<()> {
        loop {
            if stop.load(Ordering::Relaxed) != 0 {
                state = State::Stopped;
                break;
            }
            let event = input.next(&mut buffer)?;
            let at = started.elapsed();
            match event {
                Event::Data { len, skip } => {
                    state = if input.is_follow() {
                        State::Following
                    } else {
                        State::Reading
                    };
                    if skip != 0 {
                        engine.stats.add_bytes(skip as u64, at);
                        engine.stats.skipped_bytes += skip as u64;
                    }
                    engine.feed(&buffer[skip..len], at);
                }
                Event::Boundary => engine.discard_partial(),
                Event::Idle(status) => {
                    state = status;
                    if input.is_follow() {
                        thread::sleep(Duration::from_millis(20));
                    }
                }
                Event::End => {
                    engine.finish(at);
                    state = State::Complete;
                    break;
                }
            }
            if published.elapsed() >= Duration::from_millis(100) {
                publish(
                    &slot,
                    engine.snapshot(started.elapsed(), state, input.backlog()),
                );
                published = Instant::now();
            }
        }
        Ok(())
    })();
    if result.is_err() {
        state = State::Failed;
    }
    publish(
        &slot,
        engine.snapshot(started.elapsed(), state, input.backlog()),
    );
    result
}

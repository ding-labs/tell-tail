//! One in-flight webhook and one queued event; ingestion never waits on either.
use crate::webhook::{HELPER_ARG, MAX_REQUEST, REQUEST_TIMEOUT, Request, validate_url};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SHUTDOWN_GRACE: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(5);

pub struct DeliveryQueue {
    tx: mpsc::SyncSender<String>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DeliveryQueue {
    pub fn start(url: String, helper: PathBuf) -> io::Result<Self> {
        validate_url(&url).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let worker = thread::Builder::new()
            .name("tell-tail-webhooks".into())
            .spawn(move || deliver(rx, cancel, &helper, &url))?;
        Ok(Self {
            tx,
            stop,
            worker: Some(worker),
        })
    }

    /// A full queue leaves the caller free to reconsider the current breach at
    /// its next snapshot. Payloads and network responses cannot grow the queue.
    pub fn try_send(&self, body: String) -> bool {
        body.len() <= MAX_REQUEST / 2 && self.tx.try_send(body).is_ok()
    }
}

impl Drop for DeliveryQueue {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn warn(reason: &str) {
    crate::writer::diagnostic(&format!("tell-tail: webhook delivery failed ({reason})\n"));
}

fn deliver(rx: mpsc::Receiver<String>, stop: Arc<AtomicBool>, helper: &Path, url: &str) {
    let mut pending: Option<Pending> = None;
    let mut shutdown: Option<Instant> = None;
    loop {
        if stop.load(Ordering::Relaxed) {
            let since = shutdown.get_or_insert_with(Instant::now);
            if since.elapsed() >= SHUTDOWN_GRACE {
                let abandoned = pending.is_some() || rx.try_recv().is_ok();
                drop(pending.take());
                if abandoned {
                    warn("shutdown deadline reached");
                }
                return;
            }
        }
        if pending.is_none() {
            match rx.recv_timeout(POLL) {
                Ok(body) => match Pending::start(helper, url, body) {
                    Ok(request) => pending = Some(request),
                    Err(_) => warn("could not start request"),
                },
                Err(_) if shutdown.is_some() => return,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(_) => continue,
            }
        }
        if let Some(request) = &mut pending {
            if let Some(result) = request.poll() {
                drop(pending.take());
                if let Err(reason) = result {
                    warn(reason);
                }
            }
            thread::sleep(POLL);
        }
    }
}

struct Pending {
    child: Child,
    input: ChildStdin,
    packet: Vec<u8>,
    position: usize,
    started: Instant,
}

impl Pending {
    fn start(helper: &Path, url: &str, body: String) -> io::Result<Self> {
        let bytes = serde_json::to_vec(&Request {
            url: url.into(),
            body,
        })?;
        if bytes.len() > MAX_REQUEST {
            return Err(io::Error::other("webhook request too large"));
        }
        let mut packet = Vec::with_capacity(bytes.len() + 4);
        packet.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        packet.extend_from_slice(&bytes);
        let started = Instant::now();
        let mut child = Command::new(helper)
            .arg(HELPER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().expect("piped webhook stdin");
        let pending = Self {
            child,
            input,
            packet,
            position: 0,
            started,
        };
        let flags = rustix::fs::fcntl_getfl(&pending.input)?;
        rustix::fs::fcntl_setfl(&pending.input, flags | rustix::fs::OFlags::NONBLOCK)?;
        Ok(pending)
    }

    fn poll(&mut self) -> Option<Result<(), &'static str>> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                return Some(match status.code() {
                    Some(0) => Ok(()),
                    Some(10) => Err("non-2xx HTTP response"),
                    _ => Err("connection, TLS, or request error"),
                });
            }
            Err(_) => return Some(Err("could not inspect request helper")),
            Ok(None) => (),
        }
        if self.started.elapsed() >= REQUEST_TIMEOUT {
            return Some(Err("five-second request deadline reached"));
        }
        while self.position < self.packet.len() {
            match self.input.write(&self.packet[self.position..]) {
                Ok(0) => return Some(Err("request helper closed its input")),
                Ok(n) => self.position += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => return Some(Err("could not send request to helper")),
            }
        }
        None
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

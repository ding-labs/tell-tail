//! A cancellable stdout sink that never changes inherited descriptor flags.
use std::{
    io::{self, Read, Write},
    process::{Child, ChildStderr, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const HELPER_ARG: &str = "--__tell-tail-output-worker";
const CAPACITY: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(2);
const CANCEL_GRACE: Duration = Duration::from_millis(250);

pub fn is_helper() -> bool {
    let mut args = std::env::args_os().skip(1);
    args.next().is_some_and(|arg| arg == HELPER_ARG) && args.next().is_none()
}

/// Internal protocol: a little-endian u32 length, payload, then an i32 errno ACK
/// on stderr (zero on success). Only one chunk may be in flight.
pub fn run_helper() -> i32 {
    // The parent owns interactive signals and decides when output must stop.
    let ignored = Arc::new(AtomicUsize::new(0));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        if signal_hook::flag::register_usize(signal, ignored.clone(), 1).is_err() {
            return 1;
        }
    }
    let (tx, rx) = mpsc::sync_channel(0);
    if thread::Builder::new()
        .name("output-control".into())
        .spawn(move || {
            let mut input = io::stdin().lock();
            loop {
                let mut header = [0; 4];
                if input.read_exact(&mut header).is_err() {
                    // Also ends a blocked stdout write when the parent disappears.
                    std::process::exit(0);
                }
                let len = u32::from_le_bytes(header) as usize;
                if len == 0 || len > CAPACITY {
                    std::process::exit(1);
                }
                let mut payload = vec![0; len];
                if input.read_exact(&mut payload).is_err() {
                    std::process::exit(1);
                }
                if tx.send(payload).is_err() {
                    return;
                }
            }
        })
        .is_err()
    {
        return 1;
    }
    // A File writes bytes directly; it avoids stdout's terminal line buffering.
    let Ok(fd) = rustix::io::fcntl_dupfd_cloexec(io::stdout(), 3) else {
        return 1;
    };
    let mut output = std::fs::File::from(fd);
    let mut ack = io::stderr().lock();
    for payload in rx {
        let result = write_payload(&mut output, &payload);
        let errno = result
            .as_ref()
            .err()
            .and_then(io::Error::raw_os_error)
            .unwrap_or_else(|| {
                if result.is_ok() {
                    0
                } else {
                    rustix::io::Errno::IO.raw_os_error()
                }
            });
        if ack.write_all(&errno.to_le_bytes()).is_err() || result.is_err() {
            return 1;
        }
    }
    0
}

fn write_payload(output: &mut impl Write, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        match output.write(bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => bytes = &bytes[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub struct StdoutWriter {
    child: Child,
    input: ChildStdin,
    ack: ChildStderr,
    buffer: Vec<u8>,
    started: Option<Instant>,
    cancelled: Option<Instant>,
    stop: Arc<AtomicUsize>,
    failed: bool,
    discarded: bool,
}
impl StdoutWriter {
    pub fn new(stop: Arc<AtomicUsize>) -> io::Result<Self> {
        Self::spawn(stop, Stdio::inherit())
    }
    fn spawn(stop: Arc<AtomicUsize>, destination: Stdio) -> io::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(HELPER_ARG)
            .stdin(Stdio::piped())
            .stdout(destination)
            .stderr(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take().expect("piped helper stdin");
        let ack = child.stderr.take().expect("piped helper stderr");
        // These are new private IPC endpoints, never the inherited stdout.
        let setup = (|| -> io::Result<()> {
            for fd in [&input as &dyn std::os::fd::AsFd, &ack] {
                let flags = rustix::fs::fcntl_getfl(fd)?;
                rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)?;
            }
            Ok(())
        })();
        if let Err(e) = setup {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        Ok(Self {
            child,
            input,
            ack,
            buffer: Vec::with_capacity(CAPACITY),
            started: None,
            cancelled: None,
            stop,
            failed: false,
            discarded: false,
        })
    }
    /// Stop pending output before the terminal library performs its drop writes.
    /// Those writes are discarded; the terminal guard restores via a fresh sink.
    pub fn discard(&mut self) {
        self.discarded = true;
        self.buffer.clear();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn check_deadline(&mut self) -> io::Result<()> {
        if self.stop.load(Ordering::Relaxed) != 0 {
            let cancelled = self.cancelled.get_or_insert_with(Instant::now);
            if cancelled.elapsed() >= CANCEL_GRACE {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "output blocked during shutdown",
                ));
            }
        }
        if self.started.is_some_and(|t| t.elapsed() >= TIMEOUT) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "output report exceeded two seconds",
            ));
        }
        Ok(())
    }
    fn send_pending(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("output writer has failed"));
        }
        if self.buffer.is_empty() {
            return Ok(());
        }
        let result = self.exchange();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn exchange(&mut self) -> io::Result<()> {
        let header = (self.buffer.len() as u32).to_le_bytes();
        let mut position = 0;
        while position < header.len() + self.buffer.len() {
            self.check_deadline()?;
            let bytes = if position < header.len() {
                &header[position..]
            } else {
                &self.buffer[position - header.len()..]
            };
            match self.input.write(bytes) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => position += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(e) => return Err(e),
            }
        }
        let mut ack = [0; 4];
        let mut position = 0;
        while position < ack.len() {
            self.check_deadline()?;
            match self.ack.read(&mut ack[position..]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "output helper exited without acknowledgment",
                    ));
                }
                Ok(n) => position += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1))
                }
                Err(e) => return Err(e),
            }
        }
        let errno = i32::from_le_bytes(ack);
        if errno != 0 {
            return Err(io::Error::from_raw_os_error(errno));
        }
        self.buffer.clear();
        Ok(())
    }
}
impl Write for StdoutWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.discarded {
            return Ok(bytes.len());
        }
        if self.failed {
            return Err(io::Error::other("output writer has failed"));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        self.started.get_or_insert_with(Instant::now);
        self.check_deadline()?;
        if self.buffer.len() == CAPACITY {
            self.send_pending()?;
        }
        let len = bytes.len().min(CAPACITY - self.buffer.len());
        self.buffer.extend_from_slice(&bytes[..len]);
        Ok(len)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.discarded {
            return Ok(());
        }
        self.send_pending()?;
        self.started = None;
        self.cancelled = None;
        Ok(())
    }
}

/// Error reporting must not reintroduce an unbounded wait on a stalled stderr.
pub fn diagnostic(message: &str) {
    let result = (|| -> io::Result<()> {
        let fd = rustix::io::fcntl_dupfd_cloexec(io::stderr(), 3)?;
        let mut writer = StdoutWriter::spawn(Arc::new(AtomicUsize::new(1)), Stdio::from(fd))?;
        writer.write_all(message.as_bytes())?;
        writer.flush()
    })();
    let _ = result;
}
impl Drop for StdoutWriter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

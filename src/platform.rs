//! OS-specific operations live here so the parser and statistics remain portable.
#[cfg(unix)]
mod unix {
    use rustix::{
        event::{PollFd, PollFlags, Timespec, poll},
        fs::{Mode, OFlags},
    };
    use std::{
        fs::{File, Metadata},
        io,
        os::unix::fs::{FileTypeExt, MetadataExt},
        path::Path,
    };
    pub fn same_file(a: &Metadata, b: &Metadata) -> bool {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    pub fn open_regular(path: &Path) -> io::Result<File> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?;
        let file = File::from(fd);
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file input must be a regular file; pipe streams through stdin",
            ));
        }
        Ok(file)
    }
    pub fn stdin_file() -> io::Result<File> {
        Ok(File::from(rustix::io::dup(std::io::stdin())?))
    }
    pub fn readable(file: &File) -> io::Result<bool> {
        // Darwin poll does not support /dev/null; redirected regular/character
        // files can be read directly. Interactive stdin is rejected by main.
        let ty = file.metadata()?.file_type();
        if ty.is_file() || ty.is_char_device() {
            return Ok(true);
        }
        let mut fds = [PollFd::new(file, PollFlags::IN)];
        match poll(
            &mut fds,
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 100_000_000,
            }),
        ) {
            Ok(n) => Ok(n > 0),
            Err(rustix::io::Errno::INTR) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}
#[cfg(unix)]
pub use unix::*;
#[cfg(not(unix))]
compile_error!("Tell-Tail currently supports macOS and Linux; a Windows input adapter is planned.");

/// Bounded, cancellation-aware writes prevent a stalled pipe from hanging exit.
#[cfg(unix)]
pub struct StdoutWriter {
    file: std::fs::File,
    original: rustix::fs::OFlags,
    stop: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
#[cfg(unix)]
impl StdoutWriter {
    pub fn new(stop: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> std::io::Result<Self> {
        let file = std::fs::File::from(rustix::io::dup(std::io::stdout())?);
        let original = rustix::fs::fcntl_getfl(&file)?;
        rustix::fs::fcntl_setfl(&file, original | rustix::fs::OFlags::NONBLOCK)?;
        Ok(Self {
            file,
            original,
            stop,
        })
    }
}
#[cfg(unix)]
impl std::io::Write for StdoutWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let began = std::time::Instant::now();
        loop {
            match self.file.write(bytes) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if self.stop.load(std::sync::atomic::Ordering::Relaxed) != 0 {
                        // Not Interrupted: Write::write_all automatically retries that error.
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "output blocked during shutdown",
                        ));
                    }
                    if began.elapsed() >= std::time::Duration::from_secs(2) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "output remained blocked for two seconds",
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[cfg(unix)]
impl Drop for StdoutWriter {
    fn drop(&mut self) {
        let _ = rustix::fs::fcntl_setfl(&self.file, self.original);
    }
}

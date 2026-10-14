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
        Ok(File::from(rustix::io::fcntl_dupfd_cloexec(
            std::io::stdin(),
            3,
        )?))
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

pub use crate::writer::StdoutWriter;

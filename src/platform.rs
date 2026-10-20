//! OS-specific operations live here so the parser and statistics remain portable.
#[cfg(unix)]
mod unix {
    use rustix::{
        event::{PollFd, PollFlags, Timespec, poll},
        fs::{Mode, OFlags},
    };
    use std::{
        fs::{File, Metadata},
        io::{self, Read},
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
    pub struct Stdin {
        file: File,
        polled: bool,
    }
    impl Stdin {
        pub fn open() -> io::Result<Self> {
            Self::from_file(File::from(rustix::io::fcntl_dupfd_cloexec(
                std::io::stdin(),
                3,
            )?))
        }
        fn from_file(file: File) -> io::Result<Self> {
            let meta = file.metadata()?;
            let ty = meta.file_type();
            let polled = if ty.is_file()
                || (ty.is_char_device() && meta.rdev() == std::fs::metadata("/dev/null")?.rdev())
            {
                false
            } else if ty.is_fifo()
                || (ty.is_socket()
                    && rustix::net::sockopt::socket_type(&file)? == rustix::net::SocketType::STREAM)
            {
                true
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "stdin must be a regular file, pipe, stream socket, or /dev/null",
                ));
            };
            Ok(Self { file, polled })
        }
        pub fn readable(&self) -> io::Result<bool> {
            if self.polled {
                readable(&self.file)
            } else {
                Ok(true)
            }
        }
    }
    impl Read for Stdin {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.file.read(bytes)
        }
    }
    pub(crate) fn readable(file: &impl std::os::fd::AsFd) -> io::Result<bool> {
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
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            io::Write,
            os::{
                fd::OwnedFd,
                unix::net::{UnixDatagram, UnixStream},
            },
        };
        #[test]
        fn stdin_types_and_readiness_are_explicit() {
            let mut null = Stdin::from_file(File::open("/dev/null").unwrap()).unwrap();
            assert!(null.readable().unwrap());
            assert_eq!(null.read(&mut [0; 1]).unwrap(), 0);
            assert!(Stdin::from_file(File::open("/dev/zero").unwrap()).is_err());
            let (socket, mut peer) = UnixStream::pair().unwrap();
            let mut input = Stdin::from_file(File::from(OwnedFd::from(socket))).unwrap();
            assert!(!input.readable().unwrap());
            peer.write_all(b"INFO\n").unwrap();
            assert!(input.readable().unwrap());
            let mut bytes = [0; 5];
            assert_eq!(input.read(&mut bytes).unwrap(), 5);
            assert_eq!(&bytes, b"INFO\n");
            drop(peer);
            assert!(input.readable().unwrap());
            assert_eq!(input.read(&mut bytes).unwrap(), 0);
            let (datagram, _peer) = UnixDatagram::pair().unwrap();
            assert!(Stdin::from_file(File::from(OwnedFd::from(datagram))).is_err());
            let file = tempfile::tempfile().unwrap();
            let regular = Stdin::from_file(file).unwrap();
            assert!(!regular.polled);
        }
    }
}
#[cfg(unix)]
pub use unix::*;
#[cfg(not(unix))]
compile_error!("Tell-Tail currently supports macOS and Linux; a Windows input adapter is planned.");

pub use crate::writer::StdoutWriter;

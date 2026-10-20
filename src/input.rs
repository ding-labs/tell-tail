use crate::{
    cli::{Args, From},
    platform,
    stats::State,
};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
    time::{Duration, Instant},
};

mod apple;
pub use apple::AppleStream;

pub enum Event {
    Data { len: usize, skip: usize },
    Idle(State),
    Boundary,
    End,
}
pub enum Input {
    Finite(File),
    Stdin(platform::Stdin),
    Follow(Follower),
}
impl Input {
    pub fn open(args: &Args) -> io::Result<Self> {
        match args.path() {
            Some(path) if args.once => Ok(Self::Finite(platform::open_regular(path)?)),
            Some(path) => Ok(Self::Follow(Follower::open(
                path.to_owned(),
                args.from.unwrap_or(From::End),
            )?)),
            None => Ok(Self::Stdin(platform::Stdin::open()?)),
        }
    }
    pub fn next(&mut self, buffer: &mut [u8]) -> io::Result<Event> {
        match self {
            Self::Follow(f) => f.next(buffer),
            Self::Stdin(file) if !file.readable()? => Ok(Event::Idle(State::Reading)),
            Self::Stdin(file) => read_event(file, buffer),
            Self::Finite(file) => read_event(file, buffer),
        }
    }
    pub fn backlog(&self) -> Option<u64> {
        match self {
            Self::Follow(f) => f
                .file
                .as_ref()
                .and_then(|file| file.metadata().ok())
                .map(|m| m.len().saturating_sub(f.offset)),
            _ => None,
        }
    }
    pub fn is_follow(&self) -> bool {
        matches!(self, Self::Follow(_))
    }
}
fn read_event(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<Event> {
    match reader.read(buffer) {
        Ok(0) => Ok(Event::End),
        Ok(len) => Ok(Event::Data { len, skip: 0 }),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) =>
        {
            Ok(Event::Idle(State::Reading))
        }
        Err(e) => Err(e),
    }
}

pub struct Follower {
    path: PathBuf,
    file: Option<File>,
    offset: u64,
    skip_partial: bool,
    replacement: Option<(File, u64)>,
    check_at: Instant,
    state: State,
}
impl Follower {
    fn open(path: PathBuf, from: From) -> io::Result<Self> {
        let mut this = Self {
            path,
            file: None,
            offset: 0,
            skip_partial: false,
            replacement: None,
            check_at: Instant::now(),
            state: State::Following,
        };
        match platform::open_regular(&this.path) {
            Ok(mut file) => {
                if from == From::End {
                    this.offset = file.seek(SeekFrom::End(0))?;
                    if this.offset > 0 {
                        file.seek(SeekFrom::Start(this.offset - 1))?;
                        let mut last = [0];
                        // An observed concurrent truncation is handled on the next check.
                        this.skip_partial = file.read(&mut last)? == 1 && last[0] != b'\n';
                        file.seek(SeekFrom::Start(this.offset))?;
                    }
                }
                this.file = Some(file);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => this.state = State::Waiting,
            Err(e) => return Err(e),
        }
        Ok(this)
    }
    fn check(&mut self) -> io::Result<bool> {
        self.check_at = Instant::now() + Duration::from_millis(100);
        if self.replacement.is_some() {
            return Ok(false);
        }
        let path_meta = match std::fs::metadata(&self.path) {
            Ok(meta) => {
                self.state = State::Following;
                Some(meta)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.state = State::Waiting;
                None
            }
            Err(e) => return Err(e),
        };
        if let Some(file) = self.file.as_mut() {
            let meta = file.metadata()?;
            if path_meta
                .as_ref()
                .is_some_and(|p| !platform::same_file(&meta, p))
            {
                match platform::open_regular(&self.path) {
                    Ok(replacement) => self.replacement = Some((replacement, meta.len())),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => self.state = State::Waiting,
                    Err(e) => return Err(e),
                }
            }
            if meta.len() < self.offset {
                file.seek(SeekFrom::Start(0))?;
                self.offset = 0;
                self.skip_partial = false;
                return Ok(true);
            }
        } else if path_meta.is_some() {
            match platform::open_regular(&self.path) {
                Ok(file) => {
                    self.file = Some(file);
                    self.offset = 0;
                    self.skip_partial = false;
                    return Ok(true);
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => self.state = State::Waiting,
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }
    fn next(&mut self, buffer: &mut [u8]) -> io::Result<Event> {
        if Instant::now() >= self.check_at && self.check()? {
            return Ok(Event::Boundary);
        }
        if self
            .replacement
            .as_ref()
            .is_some_and(|(_, end)| self.offset >= *end)
        {
            let (next, _) = self.replacement.take().unwrap();
            self.file = Some(next);
            self.offset = 0;
            self.skip_partial = false;
            return Ok(Event::Boundary);
        }
        let limit = self
            .replacement
            .as_ref()
            .map(|(_, end)| end.saturating_sub(self.offset).min(buffer.len() as u64) as usize)
            .unwrap_or(buffer.len());
        let Some(file) = self.file.as_mut() else {
            return Ok(Event::Idle(self.state));
        };
        match file.read(&mut buffer[..limit]) {
            Ok(0) => {
                // The old file may have shrunk after rotation detection.
                if let Some((_, end)) = self.replacement.as_mut() {
                    *end = self.offset;
                }
                Ok(Event::Idle(self.state))
            }
            Ok(len) => {
                self.offset += len as u64;
                let skip = if self.skip_partial {
                    match memchr::memchr(b'\n', &buffer[..len]) {
                        Some(i) => {
                            self.skip_partial = false;
                            i + 1
                        }
                        None => len,
                    }
                } else {
                    0
                };
                Ok(Event::Data { len, skip })
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(Event::Idle(self.state)),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn read(f: &mut Follower) -> Vec<u8> {
        f.check_at = Instant::now();
        let mut out = vec![];
        let mut buf = [0; 16];
        for _ in 0..100 {
            match f.next(&mut buf).unwrap() {
                Event::Data { len, skip } => out.extend_from_slice(&buf[skip..len]),
                Event::Idle(_) => break,
                Event::Boundary => (),
                Event::End => unreachable!(),
            }
        }
        out
    }
    #[test]
    fn follows_append_rename_truncation_and_reappearance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, b"INFO old\n").unwrap();
        let mut f = Follower::open(path.clone(), From::End).unwrap();
        assert!(read(&mut f).is_empty());
        File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"WARN new\n")
            .unwrap();
        assert_eq!(read(&mut f), b"WARN new\n");
        std::fs::rename(&path, dir.path().join("old.log")).unwrap();
        std::fs::write(&path, b"ERROR replaced\n").unwrap();
        assert_eq!(read(&mut f), b"ERROR replaced\n");
        std::fs::write(&path, b"INFO\n").unwrap();
        assert_eq!(read(&mut f), b"INFO\n");
        std::fs::remove_file(&path).unwrap();
        assert!(read(&mut f).is_empty());
        assert_eq!(f.state, State::Waiting);
        std::fs::write(&path, b"FATAL\n").unwrap();
        assert_eq!(read(&mut f), b"FATAL\n");
    }
    #[test]
    fn skips_existing_partial_line_and_reads_missing_file_from_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, b"INFO par").unwrap();
        let mut f = Follower::open(path.clone(), From::End).unwrap();
        File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"tial\nWARN new\n")
            .unwrap();
        assert_eq!(read(&mut f), b"WARN new\n");
        std::fs::remove_file(&path).unwrap();
        let mut missing = Follower::open(path.clone(), From::End).unwrap();
        std::fs::write(&path, b"INFO fresh\n").unwrap();
        assert_eq!(read(&mut missing), b"INFO fresh\n");
    }
    #[test]
    fn drains_observed_generations_even_when_replaced_again_during_drain() {
        use crate::{cli::Args, engine::Engine};
        use clap::Parser;
        fn pump(f: &mut Follower, engine: &mut Engine) {
            let mut buffer = [0; 3];
            for _ in 0..100 {
                f.check_at = Instant::now();
                match f.next(&mut buffer).unwrap() {
                    Event::Data { len, skip } => {
                        assert_eq!(skip, 0);
                        engine.feed(&buffer[..len], Duration::ZERO);
                    }
                    Event::Boundary => engine.discard_partial(),
                    Event::Idle(_) => return,
                    Event::End => panic!("a follower never completes"),
                }
            }
            panic!("follower failed to reach idle");
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        let old = dir.path().join("old.log");
        std::fs::write(&path, b"INFO A\npart").unwrap();
        let mut f = Follower::open(path.clone(), From::Start).unwrap();
        let mut engine = Engine::new(&Args::parse_from(["tell-tail"]));
        let mut prefix = [0; 3];
        assert!(matches!(
            f.next(&mut prefix).unwrap(),
            Event::Data { len: 3, skip: 0 }
        ));
        engine.feed(&prefix, Duration::ZERO);
        std::fs::rename(&path, &old).unwrap();
        std::fs::write(&path, b"WARN B\n").unwrap();
        assert!(!f.check().unwrap());
        assert!(f.replacement.is_some());
        std::fs::rename(&path, dir.path().join("old-b.log")).unwrap();
        std::fs::write(&path, b"ERROR C\n").unwrap();
        pump(&mut f, &mut engine);
        let report = engine.snapshot(Duration::ZERO, State::Following, Some(0));
        assert_eq!(report.totals.records, 3);
        for level in [
            crate::parse::Level::Info,
            crate::parse::Level::Warn,
            crate::parse::Level::Error,
        ] {
            assert_eq!(report.totals.levels.get(level), 1);
        }
        assert_eq!(report.bytes_read, 26);
        assert_eq!(report.discarded_partial_records, 1);
        assert_eq!(report.skipped_bytes, 4);
        // Writes after the old descriptor's handoff are a documented loss limit.
        File::options()
            .append(true)
            .open(&old)
            .unwrap()
            .write_all(b"FATAL late\n")
            .unwrap();
        pump(&mut f, &mut engine);
        assert_eq!(engine.stats.totals.records, 3);
    }
    #[test]
    fn unobserved_truncate_and_regrow_cannot_recover_overwritten_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        std::fs::write(&path, b"INFO A\n").unwrap();
        let mut f = Follower::open(path.clone(), From::End).unwrap();
        // No observation occurs between shrinking and regrowing the same inode.
        std::fs::write(&path, b"ERROR B\n").unwrap();
        assert_eq!(read(&mut f), b"\n");
    }
}

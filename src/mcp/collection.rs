//! Fair, bounded file following with byte-offset provenance.
use super::{
    discovery::{Candidate, Discovery, Root, RootView, discover},
    model::{HISTORY_PER_SOURCE, HISTORY_TOTAL, Host, MAX_RECORD, MAX_SOURCES},
    time::now_ms,
};
use crate::{framing::Framer, platform};
use serde::Serialize;
use std::{
    fs::{File, Metadata},
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
    time::{Duration, Instant, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceView {
    pub id: u64,
    pub host: Host,
    pub name: String,
    pub generation: u64,
    pub modified_ms: i64,
    pub status: &'static str,
    pub backlog_bytes: u64,
    pub records: u64,
    pub oversized: u64,
    pub history_omitted_bytes: u64,
    pub boundaries: u64,
}
pub struct Record<'a> {
    pub source_id: u64,
    pub generation: u64,
    pub offset: u64,
    pub historical: bool,
    pub host: Host,
    pub bytes: &'a [u8],
}
struct Source {
    path: PathBuf,
    file: File,
    identity: Metadata,
    framer: Framer,
    offset: u64,
    record_offset: u64,
    historical_end: u64,
    skip_partial: bool,
    seen: bool,
    view: SourceView,
}
pub struct Collector {
    roots: Vec<Root>,
    sources: Vec<Source>,
    next_id: u64,
    history_remaining: u64,
    started_ms: i64,
    scan_at: Instant,
    pub root_views: Vec<RootView>,
    pub limited: bool,
    pub rejected: usize,
    pub retired: u64,
    pub open_failures: u64,
}
impl Collector {
    pub fn new(roots: Vec<Root>) -> Self {
        Self {
            roots,
            sources: vec![],
            next_id: 1,
            history_remaining: HISTORY_TOTAL,
            started_ms: now_ms(),
            scan_at: Instant::now(),
            root_views: vec![],
            limited: false,
            rejected: 0,
            retired: 0,
            open_failures: 0,
        }
    }
    pub fn views(&self) -> Vec<SourceView> {
        self.sources.iter().map(|s| s.view.clone()).collect()
    }
    pub fn rescan(&mut self) {
        let Discovery {
            mut candidates,
            roots,
            limited,
            rejected,
        } = discover(&self.roots);
        self.root_views = roots;
        self.limited = limited || candidates.len() > MAX_SOURCES;
        candidates.truncate(MAX_SOURCES);
        self.rejected = rejected;
        for s in &mut self.sources {
            s.seen = false;
        }
        for c in &candidates {
            if let Some(s) = self
                .sources
                .iter_mut()
                .find(|s| platform::same_file(&s.identity, &c.metadata))
            {
                s.seen = true;
                s.path = c.path.clone();
            }
        }
        // Drain a disappearing descriptor before releasing it. Its bounded
        // incomplete final record cannot be joined to a replacement file.
        self.sources.retain(|s| {
            let keep = s.seen || s.file.metadata().is_ok_and(|m| m.len() > s.offset);
            if !keep {
                self.retired += 1;
            }
            keep
        });
        for c in candidates {
            if self
                .sources
                .iter()
                .any(|s| platform::same_file(&s.identity, &c.metadata))
            {
                continue;
            }
            if self.sources.len() >= MAX_SOURCES {
                self.limited = true;
                continue;
            }
            let historical = c.modified_ms <= self.started_ms;
            match Source::open(c, self.next_id, historical, &mut self.history_remaining) {
                Ok(s) => {
                    self.next_id += 1;
                    self.sources.push(s);
                }
                Err(_) => {
                    self.open_failures += 1;
                }
            }
        }
        self.scan_at = Instant::now() + Duration::from_secs(1);
    }
    pub fn poll(&mut self, mut emit: impl FnMut(Record<'_>)) {
        if Instant::now() >= self.scan_at {
            self.rescan();
        }
        let mut buffer = [0; 64 * 1024];
        for source in &mut self.sources {
            if source.poll(&mut buffer, &mut emit).is_err() {
                source.view.status = "read failed";
            }
        }
    }
}
impl Source {
    fn open(c: Candidate, id: u64, historical: bool, budget: &mut u64) -> io::Result<Self> {
        // O_NOFOLLOW closes the final-component symlink race. Compare identity
        // with discovery as well, so a replaced file is retried on the next scan.
        let fd = rustix::fs::open(
            &c.path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        let mut file = File::from(fd);
        let identity = file.metadata()?;
        if !identity.is_file() || !platform::same_file(&identity, &c.metadata) {
            return Err(io::Error::other("source changed during discovery"));
        }
        let end = identity.len();
        let offset = if historical {
            let n = end.min(HISTORY_PER_SOURCE).min(*budget);
            *budget -= n;
            end - n
        } else {
            0
        };
        let skip_partial = if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))?;
            let mut b = [0];
            file.read_exact(&mut b)?;
            b[0] != b'\n'
        } else {
            false
        };
        file.seek(SeekFrom::Start(offset))?;
        let name = match c.host {
            Host::CodexDesktop => "Desktop log".into(),
            Host::ClaudeDesktop => super::model::label(
                c.path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("MCP log"),
            ),
        };
        Ok(Self {
            path: c.path,
            file,
            identity,
            framer: Framer::new(MAX_RECORD),
            offset,
            record_offset: offset,
            historical_end: if historical { end } else { 0 },
            skip_partial,
            seen: true,
            view: SourceView {
                id,
                host: c.host,
                name,
                generation: 0,
                modified_ms: c.modified_ms,
                status: "watching",
                backlog_bytes: end - offset,
                records: 0,
                oversized: 0,
                history_omitted_bytes: offset,
                boundaries: 0,
            },
        })
    }
    fn poll(&mut self, buffer: &mut [u8], emit: &mut impl FnMut(Record<'_>)) -> io::Result<()> {
        let metadata = self.file.metadata()?;
        self.view.modified_ms = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        if metadata.len() < self.offset {
            self.file.seek(SeekFrom::Start(0))?;
            self.offset = 0;
            self.record_offset = 0;
            self.historical_end = 0;
            self.skip_partial = false;
            self.framer.clear();
            self.view.generation += 1;
            self.view.boundaries += 1;
        }
        let n = self.file.read(buffer)?;
        self.offset += n as u64;
        self.view.backlog_bytes = metadata.len().saturating_sub(self.offset);
        self.view.status = "watching";
        let mut input = &buffer[..n];
        if self.skip_partial {
            let end = memchr::memchr(b'\n', input)
                .map(|n| n + 1)
                .unwrap_or(input.len());
            self.record_offset += end as u64;
            self.skip_partial = !input[..end].ends_with(b"\n");
            input = &input[end..];
        }
        let Self {
            framer,
            view,
            record_offset,
            historical_end,
            ..
        } = self;
        framer.feed_counted(input, |bytes, oversized, consumed| {
            let offset = *record_offset;
            *record_offset += consumed;
            view.records += 1;
            if oversized {
                view.oversized += 1;
                return;
            }
            emit(Record {
                source_id: view.id,
                generation: view.generation,
                offset,
                historical: *record_offset <= *historical_end,
                host: view.host,
                bytes,
            });
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn collector(path: &std::path::Path) -> Collector {
        Collector::new(vec![Root {
            host: Host::CodexDesktop,
            path: path.into(),
        }])
    }
    fn read(c: &mut Collector) -> Vec<(u64, u64, u64, bool, String)> {
        let mut out = vec![];
        c.poll(|r| {
            out.push((
                r.source_id,
                r.generation,
                r.offset,
                r.historical,
                String::from_utf8_lossy(r.bytes).into_owned(),
            ))
        });
        out
    }
    #[test]
    fn bootstrap_partial_appends_and_files_are_independent() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a.log");
        let b = t.path().join("b.log");
        std::fs::write(&a, "old\npar").unwrap();
        std::fs::write(&b, "other\n").unwrap();
        let mut c = collector(t.path());
        let rows = read(&mut c);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.3));
        File::options()
            .append(true)
            .open(&a)
            .unwrap()
            .write_all(b"tial\n")
            .unwrap();
        let rows = read(&mut c);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].4, "partial");
        assert_eq!(rows[0].2, 4);
        assert!(!rows[0].3);
    }
    #[test]
    fn rename_and_truncate_do_not_splice_generations() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("a.log");
        std::fs::write(&p, "old\npartial").unwrap();
        let mut c = collector(t.path());
        read(&mut c);
        std::fs::rename(&p, t.path().join("old.log")).unwrap();
        std::fs::write(&p, "new\n").unwrap();
        c.rescan();
        let rows = read(&mut c);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].4, "new");
        std::fs::write(&p, "x\n").unwrap();
        let rows = read(&mut c);
        assert_eq!(rows[0].4, "x");
        assert_eq!(rows[0].1, 1);
    }
    #[test]
    fn bootstrap_budget_skips_only_the_partial_first_record() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.log"), "omit\nkeep\n").unwrap();
        let mut c = collector(t.path());
        c.history_remaining = 6;
        let rows = read(&mut c);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].4, "keep");
        assert_eq!(rows[0].2, 5);
        assert_eq!(c.views()[0].history_omitted_bytes, 4);
    }
}

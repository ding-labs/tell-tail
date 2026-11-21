//! The Desktop name index contains titles and IDs; it has no conversation bodies.
use super::{identity::Identity, inspection::display_name};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const LIMIT: usize = 512 * 1024;

#[derive(Deserialize)]
struct Entry<'a> {
    id: &'a str,
    thread_name: String,
}

pub(super) struct Names {
    path: PathBuf,
    key: Identity,
    names: HashMap<String, String>,
    checked: Option<Instant>,
    version: Option<(u64, std::time::SystemTime)>,
}

impl Names {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            key: Identity::default(),
            names: HashMap::new(),
            checked: None,
            version: None,
        }
    }
    pub fn refresh(&mut self) {
        if self
            .checked
            .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.checked = Some(Instant::now());
        if self.load().is_err() {
            self.names.clear();
            self.version = None;
        }
    }
    fn load(&mut self) -> std::io::Result<()> {
        let mut file = crate::platform::open_regular(Path::new(&self.path))?;
        let metadata = file.metadata()?;
        let stamp = (metadata.len(), metadata.modified()?);
        if self.version == Some(stamp) {
            return Ok(());
        }
        let start = stamp.0.saturating_sub(LIMIT as u64);
        file.seek(SeekFrom::Start(start.saturating_sub(1)))?;
        let mut data = Vec::new();
        file.take(LIMIT as u64 + 1).read_to_end(&mut data)?;
        self.names.clear();
        let mut lines = data.split(|b| *b == b'\n');
        if start > 0 {
            lines.next();
        }
        for line in lines.filter(|line| line.len() <= 4096) {
            if let Ok(entry) = serde_json::from_slice::<Entry>(line) {
                let key = self.key.label("chat", entry.id);
                if let Some(name) = display_name(&entry.thread_name) {
                    self.names.insert(key, name);
                } else {
                    self.names.remove(&key);
                }
            }
        }
        self.version = Some(stamp);
        Ok(())
    }
    pub fn get(&self, id: &str) -> Option<String> {
        self.names.get(&self.key.label("chat", id)).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_exact_title_wins_and_malformed_lines_are_isolated() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("index");
        std::fs::write(&p, "{\"id\":\"a\",\"thread_name\":\"Old\"}\nbad\n{\"id\":\"b\",\"thread_name\":\"Other\"}\n{\"id\":\"a\",\"thread_name\":\"New\"}\n").unwrap();
        let mut names = Names::new(p);
        names.refresh();
        assert_eq!(names.get("a").as_deref(), Some("New"));
        assert_eq!(names.get("b").as_deref(), Some("Other"));
        assert_eq!(names.get("unknown"), None);
    }
}

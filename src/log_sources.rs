//! User-selected file monitors. Reuses the original engine, never interprets files as AI events.
use crate::{cli::Args, runtime::Running, snapshots::PublishedSnapshot};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{Arc, atomic::AtomicUsize},
};

pub const MAX_SOURCES: usize = 16;
const MAX_REGISTRY: u64 = 128 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "auto")]
    pub format: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub envelope: Option<String>,
    #[serde(default)]
    pub level_field: Option<String>,
    #[serde(default)]
    pub level_path: Option<String>,
    #[serde(default = "end")]
    pub from: String,
    #[serde(default = "window")]
    pub window: u32,
    #[serde(default = "record_limit")]
    pub max_record_bytes: u32,
}
fn auto() -> String {
    "auto".into()
}
fn end() -> String {
    "end".into()
}
fn window() -> u32 {
    60
}
fn record_limit() -> u32 {
    1_048_576
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
impl Config {
    fn args(&self) -> io::Result<Args> {
        if self.name.trim().is_empty()
            || self.name.len() > 120
            || self.name.chars().any(char::is_control)
        {
            return Err(invalid(
                "Use a name of 1–120 bytes without control characters",
            ));
        }
        if !self.path.is_absolute()
            || self.path.as_os_str().len() > 4096
            || self.path.to_string_lossy().chars().any(char::is_control)
        {
            return Err(invalid(
                "Use an absolute local file path of at most 4096 bytes",
            ));
        }
        for value in [&self.level_field, &self.level_path].into_iter().flatten() {
            if value.len() > 1024 {
                return Err(invalid("Severity selectors must be at most 1024 bytes"));
            }
        }
        let mut words = vec![
            "tell-tail".to_owned(),
            "--output=web".into(),
            format!("--format={}", self.format),
            format!("--from={}", self.from),
            format!("--window={}", self.window),
            format!("--max-record-bytes={}", self.max_record_bytes),
        ];
        for (key, value) in [
            ("profile", &self.profile),
            ("envelope", &self.envelope),
            ("level-field", &self.level_field),
            ("level-path", &self.level_path),
        ] {
            if let Some(value) = value {
                words.push(format!("--{key}={value}"));
            }
        }
        words.push("--".into());
        words.push(self.path.to_string_lossy().into_owned());
        let args = Args::try_parse_from(words).map_err(|_| {
            invalid("Invalid parsing options, window, start position or record limit")
        })?;
        args.validate().map_err(invalid)?;
        Ok(args)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    id: u64,
    enabled: bool,
    config: Config,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    next_id: u64,
    sources: Vec<Saved>,
}
struct Source {
    saved: Saved,
    running: Option<Running>,
    error: Option<String>,
}
impl Source {
    fn start(saved: Saved) -> Self {
        let mut source = Self {
            saved,
            running: None,
            error: None,
        };
        if source.saved.enabled {
            match source
                .saved
                .config
                .args()
                .and_then(|a| Running::start(&a, Arc::new(AtomicUsize::new(0))))
            {
                Ok(r) => source.running = Some(r),
                Err(e) => source.error = Some(e.to_string()),
            }
        }
        source
    }
}
pub struct Manager {
    path: Option<PathBuf>,
    _lock: Option<File>,
    next_id: u64,
    sources: Vec<Source>,
}
impl Manager {
    pub fn open(path: Option<PathBuf>) -> io::Result<Self> {
        let mut registry = Registry {
            version: 1,
            next_id: 1,
            sources: vec![],
        };
        let mut lock = None;
        if let Some(path) = &path {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .ok_or_else(|| invalid("Source registry needs an absolute path"))?;
            if !path.is_absolute() {
                return Err(invalid("Source registry needs an absolute path"));
            }
            fs::create_dir_all(parent)?;
            use std::os::unix::fs::OpenOptionsExt;
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(path.with_extension("lock"))?;
            rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
                .map_err(|_| {
                    io::Error::other("Source registry is already in use by another collector")
                })?;
            lock = Some(file);
            match crate::platform::open_regular(path) {
                Ok(file) => {
                    let mut bytes = vec![];
                    file.take(MAX_REGISTRY + 1).read_to_end(&mut bytes)?;
                    if bytes.len() as u64 > MAX_REGISTRY {
                        return Err(invalid("Source registry exceeds its size limit"));
                    }
                    registry = serde_json::from_slice(&bytes)
                        .map_err(|_| invalid("Invalid source registry; original file preserved"))?;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
        }
        if registry.version != 1 || registry.sources.len() > MAX_SOURCES || registry.next_id == 0 {
            return Err(invalid("Unsupported or oversized source registry"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for source in &registry.sources {
            if source.id == 0 || source.id >= registry.next_id || !ids.insert(source.id) {
                return Err(invalid("Invalid source identifiers"));
            }
            source.config.args()?;
        }
        Ok(Self {
            path,
            _lock: lock,
            next_id: registry.next_id,
            sources: registry.sources.into_iter().map(Source::start).collect(),
        })
    }
    fn persist(&self, sources: Vec<Saved>, next_id: u64) -> io::Result<()> {
        if let Some(path) = &self.path {
            let bytes = serde_json::to_vec_pretty(&Registry {
                version: 1,
                next_id,
                sources,
            })?;
            if bytes.len() as u64 > MAX_REGISTRY {
                return Err(invalid("Source registry exceeds its size limit"));
            }
            let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
            temp.write_all(&bytes)?;
            temp.as_file().sync_all()?;
            temp.persist(path).map_err(|e| e.error)?;
        }
        Ok(())
    }
    fn saved(&self) -> Vec<Saved> {
        self.sources.iter().map(|s| s.saved.clone()).collect()
    }
    fn validate_new(&self, config: &Config, except: Option<u64>) -> io::Result<()> {
        config.args()?;
        // Permit an absent regular file: the original follower waits for creation.
        match crate::platform::open_regular(&config.path) {
            Ok(_) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        };
        let normalized = fs::canonicalize(&config.path).unwrap_or_else(|_| config.path.clone());
        if self
            .path
            .as_ref()
            .is_some_and(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()) == normalized)
        {
            return Err(invalid("The source registry cannot monitor itself"));
        }
        if self.sources.iter().any(|s| {
            Some(s.saved.id) != except
                && fs::canonicalize(&s.saved.config.path)
                    .unwrap_or_else(|_| s.saved.config.path.clone())
                    == normalized
        }) {
            return Err(invalid("This file is already an application log source"));
        }
        Ok(())
    }
    pub fn add(&mut self, config: Config) -> io::Result<u64> {
        if self.sources.len() >= MAX_SOURCES {
            return Err(invalid(
                "At most 16 application log sources can be configured",
            ));
        }
        self.validate_new(&config, None)?;
        let id = self.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| invalid("Source identifiers exhausted"))?;
        let source = Source::start(Saved {
            id,
            enabled: true,
            config,
        });
        if let Some(error) = &source.error {
            return Err(io::Error::other(error.clone()));
        }
        let mut saved = self.saved();
        saved.push(source.saved.clone());
        self.persist(saved, next)?;
        self.sources.push(source);
        self.next_id = next;
        Ok(id)
    }
    pub fn edit(&mut self, id: u64, config: Config) -> io::Result<()> {
        self.validate_new(&config, Some(id))?;
        let index = self.index(id)?;
        let source = Source::start(Saved {
            id,
            enabled: self.sources[index].saved.enabled,
            config,
        });
        if let Some(error) = &source.error {
            return Err(io::Error::other(error.clone()));
        }
        let mut saved = self.saved();
        saved[index] = source.saved.clone();
        self.persist(saved, self.next_id)?;
        self.sources[index] = source;
        Ok(())
    }
    fn index(&self, id: u64) -> io::Result<usize> {
        self.sources
            .iter()
            .position(|s| s.saved.id == id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Source no longer exists"))
    }
    pub fn action(&mut self, id: u64, action: &str) -> io::Result<()> {
        let index = self.index(id)?;
        let mut saved = self.saved();
        match action {
            "remove" => {
                saved.remove(index);
                self.persist(saved, self.next_id)?;
                self.sources.remove(index);
            }
            "pause" => {
                saved[index].enabled = false;
                self.persist(saved.clone(), self.next_id)?;
                let s = &mut self.sources[index];
                s.saved = saved[index].clone();
                if let Some(r) = &mut s.running {
                    r.stop();
                    let _ = r.join();
                }
            }
            "resume" => {
                if self.sources[index].saved.enabled
                    && self.sources[index]
                        .running
                        .as_ref()
                        .is_some_and(|r| !r.is_finished())
                {
                    return Ok(());
                }
                saved[index].enabled = true;
                let source = Source::start(saved[index].clone());
                if let Some(error) = &source.error {
                    return Err(io::Error::other(error.clone()));
                }
                self.persist(saved, self.next_id)?;
                self.sources[index] = source;
            }
            _ => return Err(invalid("Unknown source action")),
        }
        Ok(())
    }
    pub fn publication(&self, id: u64) -> io::Result<PublishedSnapshot> {
        self.sources[self.index(id)?]
            .running
            .as_ref()
            .map(|r| r.snapshot_reader().latest())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotConnected,
                    "Source has no metrics; resume it to begin monitoring",
                )
            })
    }
    pub fn list(&self) -> serde_json::Value {
        let sources:Vec<_>=self.sources.iter().map(|s|{
            let snapshot=s.running.as_ref().map(Running::snapshot);
            let state=if !s.saved.enabled {"paused".to_owned()} else if let Some(snapshot)=&snapshot {serde_json::to_value(snapshot.state).unwrap().as_str().unwrap().to_owned()} else {"failed".to_owned()};
            serde_json::json!({"id":s.saved.id,"enabled":s.saved.enabled,"config":s.saved.config,"state":state,"error":s.error,"session_id":snapshot.as_ref().map(|s|&s.session_id)})
        }).collect();
        serde_json::json!({"persistent":self.path.is_some(),"limit":MAX_SOURCES,"sources":sources})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(path: PathBuf) -> Config {
        serde_json::from_value(serde_json::json!({"name":"App","path":path,"from":"start"}))
            .unwrap()
    }
    #[test]
    fn registry_preserves_settings_pause_and_source_identity_without_file_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        fs::write(&log, "ERROR private example\n").unwrap();
        let path = dir.path().join("sources.json");
        let mut m = Manager::open(Some(path.clone())).unwrap();
        let id = m.add(config(log.clone())).unwrap();
        assert!(Manager::open(Some(path.clone())).is_err());
        assert!(m.add(config(log.clone())).is_err());
        m.action(id, "pause").unwrap();
        assert_eq!(m.list()["sources"][0]["state"], "paused");
        drop(m);
        let mut m = Manager::open(Some(path.clone())).unwrap();
        assert_eq!(m.list()["sources"][0]["id"], id);
        assert!(!m.list()["sources"][0]["enabled"].as_bool().unwrap());
        m.action(id, "resume").unwrap();
        m.action(id, "remove").unwrap();
        drop(m);
        assert_eq!(fs::read_to_string(log).unwrap(), "ERROR private example\n");
        assert!(
            Manager::open(Some(path)).unwrap().list()["sources"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn rejects_special_files_invalid_profiles_and_corrupt_registry() {
        let mut m = Manager::open(None).unwrap();
        assert!(m.add(config("/dev/null".into())).is_err());
        let dir = tempfile::tempdir().unwrap();
        let mut c = config(dir.path().join("missing.log"));
        c.profile = Some("pino".into());
        c.format = "text".into();
        assert!(m.add(c).is_err());
        let path = dir.path().join("sources.json");
        fs::write(&path, "broken").unwrap();
        assert!(Manager::open(Some(path.clone())).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken");
    }
    #[test]
    fn failed_save_does_not_add_or_remove_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sources.json");
        let mut m = Manager::open(Some(path.clone())).unwrap();
        let id = m.add(config(dir.path().join("absent.log"))).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(m.action(id, "remove").is_err());
        assert_eq!(m.list()["sources"].as_array().unwrap().len(), 1);
    }
}

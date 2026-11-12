//! Synthetic records replayed through the same filesystem collector as real logs.
use super::{discovery::Root, model::Host, time::now_ms};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    time::{Duration, Instant},
};

pub struct Demo {
    directory: tempfile::TempDir,
    phase: u64,
    next: Instant,
}
impl Demo {
    pub fn new() -> io::Result<Self> {
        let mut demo = Self {
            directory: tempfile::tempdir()?,
            phase: 0,
            next: Instant::now(),
        };
        fs::create_dir(demo.directory.path().join("codex"))?;
        fs::create_dir(demo.directory.path().join("claude"))?;
        demo.tick()?;
        Ok(demo)
    }
    pub fn roots(&self) -> Vec<Root> {
        vec![
            Root {
                host: Host::CodexDesktop,
                path: self.directory.path().join("codex"),
            },
            Root {
                host: Host::ClaudeDesktop,
                path: self.directory.path().join("claude"),
            },
        ]
    }
    pub fn tick(&mut self) -> io::Result<()> {
        if Instant::now() < self.next {
            return Ok(());
        }
        let stamp = utc(now_ms());
        let round = self.phase / 8;
        let (status, reason, body) = match self.phase % 8 {
            0 => (
                "failed",
                "failureReason=reauthenticationRequired",
                "Initializing server...",
            ),
            1 => ("starting", "", "Server started and connected successfully"),
            2 => (
                "ready",
                "",
                "Message from client: {\"method\":\"tools/list\",\"id\":1}",
            ),
            3 => (
                "ready",
                "",
                "Server transport closed (intentional shutdown)",
            ),
            4 => (
                "failed",
                "error=\"connection timeout\"",
                "Initializing server...",
            ),
            5 => (
                "failed",
                "error=\"connection timeout\"",
                "Server transport closed unexpectedly",
            ),
            6 => ("ready", "", "Server disconnected."),
            _ => ("ready", "", "Server started and connected successfully"),
        };
        let files = [
            (
                "codex/demo.log",
                format!(
                    "{stamp} info [AppServerConnection] mcp_server_startup_status_updated server=demo-calendar status={status} {reason} threadId=demo-{round} hostId=demo\n"
                ),
            ),
            (
                "claude/mcp.log",
                format!("{stamp} [info] [demo-files] {body}\n"),
            ),
            (
                "claude/mcp-server-demo-files.log",
                format!("{stamp} [demo-files] [info] {body} {{ metadata: undefined }}\n"),
            ),
        ];
        for (name, line) in files {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(self.phase % 8 != 0)
                .truncate(self.phase % 8 == 0)
                .open(self.directory.path().join(name))?;
            file.write_all(line.as_bytes())?;
        }
        self.phase += 1;
        self.next = Instant::now() + Duration::from_secs(2);
        Ok(())
    }
}
// Gregorian civil date conversion, inverse of time::timestamp_ms.
fn utc(ms: i64) -> String {
    let seconds = ms / 1000;
    let z = seconds / 86400 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
        ms % 1000
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{collection::Collector, store::Store};
    #[test]
    fn demo_replays_failures_recovery_and_mirrors_through_files() {
        let mut demo = Demo::new().unwrap();
        let mut collector = Collector::new(demo.roots());
        let mut store = Store::default();
        collector.poll(|r| store.accept(r));
        for _ in 1..8 {
            demo.next = Instant::now();
            demo.tick().unwrap();
            collector.poll(|r| store.accept(r));
        }
        assert_eq!(store.counts.observations, 16);
        assert_eq!(store.counts.mirrors, 8);
        assert_eq!(store.issues.len(), 3);
        assert!(store.issues.iter().all(|i| i.recovered_ms.is_some()));
        for value in [0, 951782400000, now_ms(), 253402300799999] {
            assert_eq!(super::super::time::timestamp_ms(&utc(value)), Some(value));
        }
    }
}

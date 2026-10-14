use crate::{
    parse::Level,
    platform,
    stats::{Snapshot, State},
};
use std::{
    fmt::{Display, Write as _},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn render(s: &Snapshot) -> String {
    let mut out = String::with_capacity(3000);
    out.push_str("# HELP tell_tail_records_total Observed physical log records by severity.\n# TYPE tell_tail_records_total counter\n");
    for level in Level::ALL {
        writeln!(
            out,
            "tell_tail_records_total{{level=\"{}\"}} {}",
            level.name(),
            s.totals.levels.get(level)
        )
        .unwrap();
    }
    scalar(
        &mut out,
        "tell_tail_bytes_total",
        "counter",
        "Bytes consumed including delimiters and skipped data.",
        s.bytes_read,
    );
    scalar(
        &mut out,
        "tell_tail_parse_failures_total",
        "counter",
        "Malformed JSON records.",
        s.totals.parse_failures,
    );
    scalar(
        &mut out,
        "tell_tail_oversized_records_total",
        "counter",
        "Records exceeding the configured limit.",
        s.totals.oversized_records,
    );
    scalar(
        &mut out,
        "tell_tail_skipped_bytes_total",
        "counter",
        "Consumed bytes excluded from record classification.",
        s.skipped_bytes,
    );
    scalar(
        &mut out,
        "tell_tail_discarded_partial_records_total",
        "counter",
        "Partial records discarded at file generation boundaries.",
        s.discarded_partial_records,
    );
    scalar(
        &mut out,
        "tell_tail_pending_bytes",
        "gauge",
        "Bytes in an incomplete record.",
        s.pending_bytes,
    );
    if let Some(backlog) = s.backlog_bytes {
        scalar(
            &mut out,
            "tell_tail_backlog_bytes",
            "gauge",
            "Unread bytes in the current followed file.",
            backlog,
        );
    }
    scalar(
        &mut out,
        "tell_tail_running",
        "gauge",
        "One while processing or waiting for input; zero after termination.",
        u8::from(!s.state.is_final()),
    );
    scalar(
        &mut out,
        "tell_tail_source_up",
        "gauge",
        "One while the input is available; zero while missing or terminated.",
        u8::from(matches!(s.state, State::Reading | State::Following)),
    );
    scalar(
        &mut out,
        "tell_tail_snapshot_timestamp_seconds",
        "gauge",
        "Unix time at which this snapshot was produced.",
        s.observed_at_unix_seconds,
    );
    scalar(
        &mut out,
        "tell_tail_window_duration_seconds",
        "gauge",
        "Actual time covered by the rolling window.",
        s.window.observed_seconds,
    );
    scalar(
        &mut out,
        "tell_tail_window_warning_ratio",
        "gauge",
        "Warning fraction in the rolling window; NaN when empty.",
        s.window.warning_ratio.unwrap_or(f64::NAN),
    );
    scalar(
        &mut out,
        "tell_tail_window_error_ratio",
        "gauge",
        "Error and fatal fraction in the rolling window; NaN when empty.",
        s.window.error_ratio.unwrap_or(f64::NAN),
    );
    out
}
fn scalar(out: &mut String, name: &str, kind: &str, help: &str, value: impl Display) {
    writeln!(
        out,
        "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
    )
    .unwrap();
}

pub struct MetricsFile {
    path: PathBuf,
    input: Option<PathBuf>,
}
impl MetricsFile {
    pub fn new(path: PathBuf, input: Option<PathBuf>) -> io::Result<Self> {
        let file = Self { path, input };
        file.validate_destination()?;
        Ok(file)
    }
    fn parent(&self) -> &Path {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    }
    fn validate_destination(&self) -> io::Result<()> {
        let target = self
            .parent()
            .canonicalize()?
            .join(self.path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "metrics file needs a filename")
            })?);
        if let Some(input) = &self.input {
            let input_parent = input
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            if input_parent
                .canonicalize()
                .ok()
                .zip(input.file_name())
                .is_some_and(|(p, name)| p.join(name) == target)
                || std::fs::metadata(input)
                    .ok()
                    .zip(std::fs::metadata(&self.path).ok())
                    .is_some_and(|(a, b)| platform::same_file(&a, &b))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "metrics output must not replace the input log",
                ));
            }
        }
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| !m.is_file()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "metrics destination must be a regular file",
            ));
        }
        Ok(())
    }
    pub fn publish(&self, snapshot: &Snapshot) -> io::Result<()> {
        self.validate_destination()?;
        let mut temp = tempfile::Builder::new()
            .prefix(".tell-tail-")
            .tempfile_in(self.parent())?;
        // Preserve existing access permissions; new files can be read by a collector.
        let permissions = std::fs::metadata(&self.path)
            .map(|m| m.permissions())
            .unwrap_or_else(|_| {
                use std::os::unix::fs::PermissionsExt;
                std::fs::Permissions::from_mode(0o644)
            });
        temp.as_file().set_permissions(permissions)?;
        temp.write_all(render(snapshot).as_bytes())?;
        temp.flush()?;
        temp.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse::Classification, stats::Stats};
    use std::time::Duration;
    #[test]
    fn exposition_has_unique_series_and_all_severities() {
        let mut stats = Stats::new(60, "test".into());
        stats.record(
            Classification {
                level: Level::Warn,
                parse_failure: false,
            },
            false,
            Duration::ZERO,
        );
        let text = render(&stats.snapshot(Duration::from_secs(1), State::Following, 0, Some(0)));
        assert!(text.contains("tell_tail_records_total{level=\"warn\"} 1\n"));
        assert!(text.contains("tell_tail_records_total{level=\"fatal\"} 0\n"));
        let mut names = std::collections::HashSet::new();
        for line in text.lines().filter(|s| !s.starts_with('#')) {
            let mut tokens = line.split_whitespace();
            assert!(names.insert(tokens.next().unwrap()));
            assert!(tokens.next().unwrap().parse::<f64>().is_ok());
            assert!(tokens.next().is_none());
        }
    }
    #[test]
    fn atomic_file_replaces_snapshots_and_rejects_input_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("app.log");
        std::fs::write(&input, "INFO hello\n").unwrap();
        assert!(MetricsFile::new(dir.path().join("./app.log"), Some(input.clone())).is_err());
        let alias = dir.path().join("alias.prom");
        std::fs::hard_link(&input, &alias).unwrap();
        assert!(MetricsFile::new(alias, Some(input.clone())).is_err());
        let destination = dir.path().join("metrics.prom");
        let file = MetricsFile::new(destination.clone(), Some(input)).unwrap();
        let mut stats = Stats::new(60, "test".into());
        let s = stats.snapshot(Duration::from_secs(1), State::Following, 0, None);
        file.publish(&s).unwrap();
        file.publish(&s).unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), render(&s));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    }
}

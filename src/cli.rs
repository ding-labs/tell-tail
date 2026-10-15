use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Auto,
    Text,
    Json,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Output {
    Auto,
    Tui,
    Text,
    Json,
    Prometheus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum From {
    Start,
    End,
}

/// Measure log throughput and severity with bounded memory.
#[derive(Clone, Debug, Parser)]
#[command(version, about)]
pub struct Args {
    /// File to follow; omit or use - to read stdin.
    pub file: Option<PathBuf>,
    /// Read a file from the beginning and exit at EOF.
    #[arg(long)]
    pub once: bool,
    /// Initial position when following a file (default: end).
    #[arg(long, value_enum, conflicts_with = "once")]
    pub from: Option<From>,
    #[arg(long, value_enum, default_value = "auto")]
    pub format: Format,
    /// Top-level JSON severity field.
    #[arg(long, default_value = "level")]
    pub level_field: String,
    /// Rolling window in seconds (1..=3600).
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=3600))]
    pub window: u32,
    #[arg(long, value_enum, default_value = "auto")]
    pub output: Output,
    /// Atomically publish Prometheus snapshots to this file.
    #[arg(long)]
    pub metrics_file: Option<PathBuf>,
    /// Maximum buffered record size in bytes (1..=16 MiB).
    #[arg(long, default_value_t = 1_048_576, value_parser = clap::value_parser!(u32).range(1..=16_777_216))]
    pub max_record_bytes: u32,
    /// Text, JSON, and metrics-file publication interval in milliseconds.
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(100..=60_000))]
    pub interval_ms: u32,
    /// POST alert JSON to this HTTP(S) endpoint.
    #[arg(long, requires = "alert_error_percent")]
    pub alert_webhook: Option<String>,
    /// Alert when rolling (error + fatal) / all records is at least this percent.
    #[arg(long, requires = "alert_webhook")]
    pub alert_error_percent: Option<f64>,
    /// Message included in the alert JSON (at most 4096 bytes).
    #[arg(long, requires = "alert_webhook")]
    pub alert_message: Option<String>,
    /// Minimum records in the rolling window before evaluating an alert.
    #[arg(long, default_value_t = 100, requires = "alert_webhook", value_parser = clap::value_parser!(u64).range(1..))]
    pub alert_min_records: u64,
    /// Minimum seconds between new breaches; sustained breaches only fire once.
    #[arg(long, default_value_t = 300, requires = "alert_webhook", value_parser = clap::value_parser!(u32).range(0..=86_400))]
    pub alert_cooldown: u32,
}
impl Args {
    pub fn path(&self) -> Option<&std::path::Path> {
        self.file
            .as_deref()
            .filter(|p| *p != std::path::Path::new("-"))
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .alert_error_percent
            .is_some_and(|p| !p.is_finite() || !(0.0..=100.0).contains(&p))
        {
            return Err("--alert-error-percent must be a finite number from 0 to 100");
        }
        if let Some(url) = &self.alert_webhook {
            crate::webhook::validate_url(url)?;
        }
        if self.alert_message.as_ref().is_some_and(|m| m.len() > 4096) {
            return Err("--alert-message must be at most 4096 bytes");
        }
        if self.path().is_none() && (self.once || self.from.is_some()) {
            return Err("--once and --from require a file input");
        }
        if self.level_field.is_empty() {
            return Err("--level-field must not be empty");
        }
        if self.metrics_file.is_some() && self.output != Output::Prometheus {
            return Err("--metrics-file requires --output prometheus");
        }
        if self.output == Output::Prometheus
            && self.path().is_some()
            && !self.once
            && self.metrics_file.is_none()
        {
            return Err("following in Prometheus mode requires --metrics-file");
        }
        if self.metrics_file.as_deref() == self.path() && self.path().is_some() {
            return Err("metrics output must differ from the input file");
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_combinations() {
        for argv in [
            vec!["tell-tail", "--once"],
            vec!["tell-tail", "a.log", "--output", "prometheus"],
            vec!["tell-tail", "--metrics-file", "a.prom"],
        ] {
            assert!(Args::try_parse_from(argv).unwrap().validate().is_err());
        }
        assert!(Args::try_parse_from(["tell-tail", "a.log", "--once", "--from", "start"]).is_err());
        assert!(Args::try_parse_from(["tell-tail", "--window", "0"]).is_err());
    }
    #[test]
    fn accepts_supported_modes() {
        for argv in [
            vec!["tell-tail"],
            vec!["tell-tail", "-"],
            vec!["tell-tail", "a.log"],
            vec!["tell-tail", "a.log", "--once", "--output", "prometheus"],
        ] {
            assert!(Args::try_parse_from(argv).unwrap().validate().is_ok());
        }
    }

    #[test]
    fn rejects_invalid_alert_configuration_without_echoing_endpoints() {
        for percent in ["NaN", "inf", "101", "-0.1"] {
            let args = Args::try_parse_from([
                "tell-tail",
                "--alert-webhook",
                "http://localhost/hook",
                &format!("--alert-error-percent={percent}"),
            ])
            .unwrap();
            assert!(args.validate().is_err());
        }
        let args = Args::try_parse_from([
            "tell-tail",
            "--alert-webhook",
            "file:///secret-token",
            "--alert-error-percent",
            "25",
        ])
        .unwrap();
        let error = args.validate().unwrap_err();
        assert!(!error.contains("secret-token"));
        let args = Args::try_parse_from([
            "tell-tail",
            "--alert-webhook",
            "http://localhost",
            "--alert-error-percent",
            "25",
            "--alert-message",
            &"x".repeat(4097),
        ])
        .unwrap();
        assert!(args.validate().is_err());
        for extra in [["--alert-min-records", "0"], ["--alert-cooldown", "86401"]] {
            assert!(
                Args::try_parse_from(
                    [
                        "tell-tail",
                        "--alert-webhook",
                        "http://localhost",
                        "--alert-error-percent",
                        "25"
                    ]
                    .into_iter()
                    .chain(extra)
                )
                .is_err()
            );
        }
    }
}

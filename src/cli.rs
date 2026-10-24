use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Auto,
    Text,
    Json,
    /// Apple unified log NDJSON; maps messageType and skips stream metadata.
    AppleUnified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Output {
    Auto,
    Tui,
    Text,
    Json,
    Prometheus,
    /// Serve local metrics; stdin EOF keeps the server available until Ctrl+C.
    #[cfg(feature = "web")]
    Web,
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
    /// Stream macOS unified logs at info level; no pipe or jq required.
    #[arg(long, conflicts_with_all = ["file", "once", "from"])]
    pub apple_unified: bool,
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
    /// Local web port (0 selects an available port); requires --output web.
    #[cfg(feature = "web")]
    #[arg(long)]
    pub web_port: Option<u16>,
    /// Open the local dashboard in the default browser; requires --output web.
    #[cfg(feature = "web")]
    #[arg(long)]
    pub open: bool,
    /// Atomically publish Prometheus snapshots to this file.
    #[arg(long)]
    pub metrics_file: Option<PathBuf>,
    /// Maximum buffered record size in bytes (1..=16 MiB).
    #[arg(long, default_value_t = 1_048_576, value_parser = clap::value_parser!(u32).range(1..=16_777_216))]
    pub max_record_bytes: u32,
    /// Text, JSON, and metrics-file publication interval in milliseconds.
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(100..=60_000))]
    pub interval_ms: u32,
    /// POST breach/recovery JSON; use --slack or --discord for chat webhooks.
    #[arg(long, requires = "alert_error_percent")]
    pub alert_webhook: Option<String>,
    /// Format alerts for a Slack incoming webhook; requires --alert-webhook.
    #[arg(long, requires = "alert_webhook")]
    pub slack: bool,
    /// Format alerts for a Discord webhook; requires --alert-webhook.
    #[arg(long, requires = "alert_webhook", conflicts_with = "slack")]
    pub discord: bool,
    /// Alert when rolling (error + fatal) / all records is at least this percent.
    #[arg(long, requires = "alert_webhook")]
    pub alert_error_percent: Option<f64>,
    /// Message included in breach notifications (at most 4096 bytes).
    #[arg(long, requires = "alert_webhook")]
    pub alert_message: Option<String>,
    /// Minimum records in the rolling window to evaluate a breach or recovery.
    #[arg(long, default_value_t = 100, requires = "alert_webhook", value_parser = clap::value_parser!(u64).range(1..))]
    pub alert_min_records: u64,
    /// Minimum seconds between new breaches; sustained breaches only fire once.
    #[arg(long, default_value_t = 300, requires = "alert_webhook", value_parser = clap::value_parser!(u32).range(0..=86_400))]
    pub alert_cooldown: u32,
}
impl Args {
    pub fn input_format(&self) -> Format {
        if self.apple_unified {
            Format::AppleUnified
        } else {
            self.format
        }
    }
    pub fn path(&self) -> Option<&std::path::Path> {
        self.file
            .as_deref()
            .filter(|p| *p != std::path::Path::new("-"))
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.apple_unified {
            if !cfg!(target_os = "macos") {
                return Err(
                    "--apple-unified live capture requires macOS; use --format apple-unified for saved NDJSON",
                );
            }
            if !matches!(self.format, Format::Auto | Format::AppleUnified) {
                return Err("--apple-unified selects the apple-unified format; omit --format");
            }
        }
        if self.input_format() == Format::AppleUnified
            && !matches!(self.level_field.as_str(), "level" | "messageType")
        {
            return Err("--format apple-unified uses messageType; omit --level-field");
        }
        #[cfg(feature = "web")]
        {
            if self.web_port.is_some() && self.output != Output::Web {
                return Err("--web-port requires --output web");
            }
            if self.open && self.output != Output::Web {
                return Err("--open requires --output web");
            }
            if self.once && self.output == Output::Web {
                return Err(
                    "--once cannot be combined with --output web; web serves final stdin metrics until Ctrl+C",
                );
            }
        }
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
    fn apple_source_and_capture_format_have_explicit_contracts() {
        let args = Args::parse_from(["tell-tail", "--apple-unified"]);
        assert_eq!(args.input_format(), Format::AppleUnified);
        assert_eq!(args.validate().is_ok(), cfg!(target_os = "macos"));
        for extra in [
            vec!["capture.ndjson"],
            vec!["-"],
            vec!["--once"],
            vec!["--from", "start"],
        ] {
            assert!(
                Args::try_parse_from([vec!["tell-tail", "--apple-unified"], extra].concat())
                    .is_err()
            );
        }
        assert!(
            Args::parse_from(["tell-tail", "--apple-unified", "--format", "text"])
                .validate()
                .is_err()
        );
        assert!(
            Args::parse_from([
                "tell-tail",
                "--format",
                "apple-unified",
                "--level-field",
                "severity"
            ])
            .validate()
            .is_err()
        );
        assert!(
            Args::parse_from([
                "tell-tail",
                "capture.ndjson",
                "--once",
                "--format",
                "apple-unified"
            ])
            .validate()
            .is_ok()
        );
    }
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

    #[cfg(feature = "web")]
    #[test]
    fn web_arguments_keep_eof_and_port_contracts_explicit() {
        assert!(
            Args::try_parse_from(["tell-tail", "--output", "web", "--open"])
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            Args::try_parse_from(["tell-tail", "--open"])
                .unwrap()
                .validate()
                .is_err()
        );
        for port in ["0", "8765", "65535"] {
            assert!(
                Args::try_parse_from(["tell-tail", "--output", "web", "--web-port", port])
                    .unwrap()
                    .validate()
                    .is_ok()
            );
        }
        assert!(
            Args::try_parse_from(["tell-tail", "--output", "web", "--web-port", "65536"]).is_err()
        );
        assert!(
            Args::try_parse_from(["tell-tail", "--web-port", "0"])
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            Args::try_parse_from(["tell-tail", "input.log", "--output", "web", "--once"])
                .unwrap()
                .validate()
                .is_err()
        );
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

    #[test]
    fn chat_formats_require_a_webhook_and_threshold_and_are_opt_in() {
        let defaults = Args::parse_from(["tell-tail"]);
        assert!(!defaults.slack && !defaults.discord);
        let base = [
            "tell-tail",
            "--alert-webhook",
            "https://example.com/hook",
            "--alert-error-percent",
            "25",
        ];
        let generic = Args::parse_from(base);
        assert!(!generic.slack && !generic.discord);
        for flag in ["--slack", "--discord"] {
            for extra in [
                vec![],
                vec!["--alert-error-percent", "25"],
                vec!["--alert-webhook", "https://example.com/hook"],
            ] {
                assert!(
                    Args::try_parse_from(["tell-tail", flag].into_iter().chain(extra)).is_err()
                );
            }
            let args = Args::parse_from(base.into_iter().chain([flag]));
            assert_eq!(args.slack, flag == "--slack");
            assert_eq!(args.discord, flag == "--discord");
            assert!(args.validate().is_ok());
        }
        for flags in [["--slack", "--discord"], ["--discord", "--slack"]] {
            let error = Args::try_parse_from(base.into_iter().chain(flags)).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }
}

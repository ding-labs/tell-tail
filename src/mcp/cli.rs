use super::{
    discovery::{self, Root},
    model::Host,
};
use clap::{Parser, ValueEnum};
use std::{io, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Output {
    Text,
    Json,
    Web,
}
/// Watch existing MCP diagnostics without changing server configuration.
#[derive(Debug, Parser)]
#[command(name = "tell-tail mcp", version)]
pub struct Args {
    /// Open the local dashboard (implies --output web).
    #[arg(long)]
    pub open: bool,
    #[arg(long, value_enum)]
    pub output: Option<Output>,
    /// Restrict discovery to one supported desktop host.
    #[arg(long, value_enum)]
    pub source: Option<Host>,
    /// Read only this directory using the selected --source adapter.
    #[arg(long, requires = "source")]
    pub log_dir: Option<PathBuf>,
    /// Loopback port (default: choose an available port).
    #[arg(long)]
    pub web_port: Option<u16>,
    /// Inspect bounded existing history and exit; headless outputs only.
    #[arg(long)]
    pub once: bool,
    /// Replay synthetic logs in temporary files; do not read client logs.
    #[arg(long, conflicts_with_all = ["source", "log_dir"])]
    pub demo: bool,
    /// Replay a finite normalized tool-trace fixture (text/JSON only; no client logs).
    #[arg(long, conflicts_with_all = ["source", "log_dir", "demo"])]
    pub trace_replay: Option<PathBuf>,
    /// Receive Codex OTLP results on a separate loopback port (0 chooses a free port).
    #[arg(long, conflicts_with_all = ["once", "demo", "trace_replay"], requires = "trace_token_file")]
    pub trace_port: Option<u16>,
    /// Private local credential file; created if absent. Never pass the credential itself.
    #[arg(long, requires = "trace_port")]
    pub trace_token_file: Option<PathBuf>,
    /// Write a new private exporter fragment for manual client setup; never edits client settings.
    #[arg(long, requires = "trace_port")]
    pub trace_config_out: Option<PathBuf>,
}
impl Args {
    pub fn mode(&self) -> Output {
        self.output
            .unwrap_or(if self.open { Output::Web } else { Output::Text })
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.trace_port.is_some() && !cfg!(feature = "web") {
            return Err("Live trace capture requires the web feature; replay remains available");
        }
        if self.trace_replay.is_some() && self.mode() == Output::Web {
            return Err(
                "Trace replay currently supports --output text or json; live capture is not yet verified",
            );
        }
        if self.open && self.mode() != Output::Web {
            return Err("--open requires web output; omit --output or use --output web");
        }
        if self.web_port.is_some() && self.mode() != Output::Web {
            return Err("--web-port requires --open or --output web");
        }
        if self.once && self.mode() == Output::Web {
            return Err("--once requires text or json output");
        }
        if self.mode() == Output::Web && !cfg!(feature = "web") {
            return Err(
                "MCP web output requires a build with the web feature; use --output text or json",
            );
        }
        Ok(())
    }
    pub fn roots(&self) -> io::Result<Vec<Root>> {
        if let Some(path) = &self.log_dir {
            return Ok(vec![Root {
                host: self.source.expect("clap requires source"),
                path: path.clone(),
            }]);
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| io::Error::other("HOME is unavailable; use --source and --log-dir"))?;
        Ok(discovery::defaults(&PathBuf::from(home))
            .into_iter()
            .filter(|r| self.source.is_none_or(|h| r.host == h))
            .collect())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_is_convenient_and_conflicts_are_explicit() {
        let a = Args::parse_from(["mcp", "--open"]);
        assert_eq!(a.mode(), Output::Web);
        assert!(
            Args::parse_from(["mcp", "--open", "--output", "json"])
                .validate()
                .is_err()
        );
        assert!(Args::try_parse_from(["mcp", "--log-dir", "."]).is_err());
        assert!(Args::try_parse_from(["mcp", "--alert-webhook", "http://example"]).is_err());
        assert!(Args::parse_from(["mcp", "--once"]).validate().is_ok());
    }
}

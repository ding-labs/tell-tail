use clap::ValueEnum;
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SOURCES: usize = 64;
pub const MAX_RECORD: usize = 256 * 1024;
pub const MAX_EVENTS: usize = 2000;
pub const MAX_ISSUES: usize = 1024;
pub const HISTORY_PER_SOURCE: u64 = 2 * 1024 * 1024;
pub const HISTORY_TOTAL: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Host {
    CodexDesktop,
    ClaudeDesktop,
}
impl Host {
    pub fn label(self) -> &'static str {
        match self {
            Self::CodexDesktop => "Codex Desktop",
            Self::ClaudeDesktop => "Claude Desktop",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Starting,
    Ready,
    Failed,
    Cancelled,
    DiscoveryFailed,
    DiagnosticFailed,
    Closed,
    IntentionalClose,
    RendererRelease,
    UnexpectedClose,
    Protocol,
    Unknown,
}
impl Kind {
    pub fn is_failure(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::DiscoveryFailed | Self::DiagnosticFailed | Self::UnexpectedClose
        )
    }
    pub fn phase(self) -> &'static str {
        match self {
            Self::DiscoveryFailed => "discovery",
            Self::DiagnosticFailed => "diagnostic",
            Self::Closed
            | Self::IntentionalClose
            | Self::RendererRelease
            | Self::UnexpectedClose => "connection",
            _ => "startup",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "Server starting",
            Self::Ready => "Server reported ready",
            Self::Failed => "Server startup failed",
            Self::Cancelled => "Server startup cancelled",
            Self::DiscoveryFailed => "Tool discovery failed",
            Self::DiagnosticFailed => "Server reported an error",
            Self::Closed => "Connection closed; outcome unknown",
            Self::IntentionalClose => "Intentional shutdown",
            Self::RendererRelease => "Renderer released connection",
            Self::UnexpectedClose => "Unexpected server disconnect",
            Self::Protocol => "Protocol metadata; payload omitted",
            Self::Unknown => "Unrecognized MCP record; content omitted",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Reauthentication,
    Authentication,
    Timeout,
    MissingExecutable,
    Transport,
    Other,
}
impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reauthentication => "OAuth reauthentication required",
            Self::Authentication => "Authentication required or rejected",
            Self::Timeout => "Host reported a timeout",
            Self::MissingExecutable => "Executable or file not found",
            Self::Transport => "Connection or transport failure",
            Self::Other => "Failure reported by host",
        }
    }
    pub fn advice(self) -> &'static str {
        match self {
            Self::Reauthentication | Self::Authentication => {
                "Sign in to this server again in the host application."
            }
            Self::Timeout => "Check the server's availability and the host's timeout settings.",
            Self::MissingExecutable => {
                "Check that the configured executable and required files exist."
            }
            Self::Transport => "Check server availability and network connectivity.",
            Self::Other => "Review this server's diagnostics in the host application.",
        }
    }
}

/// Parser output is internal. Never serialize original payloads or execution IDs.
#[derive(Clone, Debug)]
pub struct Parsed {
    pub host: Host,
    pub server: String,
    pub scope: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub level: &'static str,
    pub kind: Kind,
    pub reason: Option<Reason>,
    pub method: Option<&'static str>,
    pub fingerprint: u64,
    pub aggregate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Evidence {
    pub source_id: u64,
    pub generation: u64,
    pub offset: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: u64,
    pub host: Host,
    pub server: String,
    /// Process-local opaque scope. Never a host conversation ID.
    pub scope: String,
    pub scope_known: bool,
    pub timestamp_ms: Option<i64>,
    pub observed_ms: i64,
    pub historical: bool,
    pub level: &'static str,
    pub kind: Kind,
    pub reason: Option<Reason>,
    pub summary: String,
    pub method: Option<&'static str>,
    pub evidence: Vec<Evidence>,
    pub payload_available: bool,
    #[serde(skip)]
    pub fingerprint: u64,
    #[serde(skip)]
    pub aggregate: bool,
}

/// Names are labels, not arbitrary evidence. Reject paths, URLs, control bytes,
/// long values, and credential-shaped names rather than forwarding their text.
pub fn label(value: &str) -> String {
    if value.is_empty()
        || value.len() > 96
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " ._-()".contains(c))
        || ["sk-", "sk_", "ghp_", "github_pat_", "eyJ"]
            .iter()
            .any(|p| value.starts_with(p))
    {
        "unknown-server".into()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_labels_cannot_carry_payloads_paths_or_credentials() {
        for name in [
            "https://host?token=secret",
            "/Users/private/server",
            "x\nERROR",
            "sk-private",
            "eyJprivate",
            "<script>",
        ] {
            assert_eq!(label(name), "unknown-server");
        }
        assert_eq!(label("PDF (Example)"), "PDF (Example)");
        assert!(!Kind::Closed.is_failure());
        assert!(!Kind::Cancelled.is_failure());
        assert!(Kind::UnexpectedClose.is_failure());
    }
}

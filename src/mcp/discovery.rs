//! Shallow, bounded discovery. No configuration or conversation files are read.
use super::{model::Host, time::now_ms};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_VISITS: usize = 4096;
const MAX_CANDIDATES: usize = 512;

#[derive(Clone, Debug)]
pub struct Root {
    pub host: Host,
    pub path: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RootView {
    pub host: Host,
    pub location: String,
    pub status: &'static str,
}
#[derive(Debug)]
pub struct Candidate {
    pub host: Host,
    pub path: PathBuf,
    pub metadata: fs::Metadata,
    pub modified_ms: i64,
}
#[derive(Default)]
pub struct Discovery {
    pub candidates: Vec<Candidate>,
    pub roots: Vec<RootView>,
    pub limited: bool,
    pub rejected: usize,
}

pub fn defaults(home: &Path) -> Vec<Root> {
    if !cfg!(target_os = "macos") {
        return vec![];
    }
    vec![
        Root {
            host: Host::CodexDesktop,
            path: home.join("Library/Logs/com.openai.codex"),
        },
        Root {
            host: Host::ClaudeDesktop,
            path: home.join("Library/Logs/Claude"),
        },
    ]
}

fn log_name(host: Host, name: &str) -> bool {
    if name.len() > 255 {
        return false;
    }
    match host {
        Host::CodexDesktop => name.ends_with(".log"),
        Host::ClaudeDesktop => {
            let base = name.strip_suffix(".log").unwrap_or("");
            base == "mcp"
                || base
                    .strip_prefix("mcp")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                || base.starts_with("mcp-server-")
        }
    }
}

fn location(root: &Root) -> String {
    let suffix = match root.host {
        Host::CodexDesktop => "Library/Logs/com.openai.codex",
        Host::ClaudeDesktop => "Library/Logs/Claude",
    };
    if root.path.ends_with(suffix) {
        format!("~/{suffix}")
    } else {
        "Custom log directory".into()
    }
}

pub fn discover(roots: &[Root]) -> Discovery {
    let mut out = Discovery::default();
    for root in roots.iter().take(2) {
        // Reserve traversal and candidate capacity for each host. A large Codex
        // archive must not prevent Claude's older dedicated logs being seen.
        let mut visits = MAX_VISITS - MAX_VISITS / roots.len().clamp(1, 2);
        let mut found = Discovery::default();
        let mut view = RootView {
            host: root.host,
            location: location(root),
            status: "watching",
        };
        match fs::canonicalize(&root.path) {
            Ok(path) if path.is_dir() => scan(
                &path,
                &path,
                root.host,
                0,
                &mut visits,
                &mut found,
                &mut view.status,
            ),
            Ok(_) => view.status = "not a directory",
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                view.status = "waiting for directory"
            }
            Err(_) => view.status = "directory inaccessible",
        }
        out.limited |= found.limited;
        out.rejected += found.rejected;
        out.candidates.extend(found.candidates);
        out.roots.push(view);
    }
    out.candidates.sort_by(|a, b| {
        b.modified_ms
            .cmp(&a.modified_ms)
            .then_with(|| a.path.cmp(&b.path))
    });
    // Round-robin host ranks retain freshness within a host and fair coverage
    // across hosts at both the candidate and active-source limits.
    let mut ranks = [0usize; 2];
    let mut ranked: Vec<_> = out
        .candidates
        .drain(..)
        .map(|candidate| {
            let host = usize::from(candidate.host == Host::ClaudeDesktop);
            let rank = ranks[host];
            ranks[host] += 1;
            ((rank, host), candidate)
        })
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    out.candidates = ranked.into_iter().map(|(_, candidate)| candidate).collect();
    if out.candidates.len() > MAX_CANDIDATES {
        out.candidates.truncate(MAX_CANDIDATES);
        out.limited = true;
    }
    out
}

fn scan(
    root: &Path,
    directory: &Path,
    host: Host,
    depth: usize,
    visits: &mut usize,
    out: &mut Discovery,
    status: &mut &'static str,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        *status = "some directories inaccessible";
        return;
    };
    let mut dirs = Vec::new();
    for entry in entries {
        if *visits >= MAX_VISITS {
            out.limited = true;
            break;
        }
        *visits += 1;
        let Ok(entry) = entry else {
            out.rejected += 1;
            continue;
        };
        let Ok(ty) = entry.file_type() else {
            out.rejected += 1;
            continue;
        };
        if ty.is_symlink() {
            out.rejected += 1;
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if ty.is_dir()
            && host == Host::CodexDesktop
            && depth < 3
            && name.len() == if depth == 0 { 4 } else { 2 }
            && name.bytes().all(|b| b.is_ascii_digit())
        {
            dirs.push(entry.path());
        } else if ty.is_file() && log_name(host, name) {
            let Ok(path) = fs::canonicalize(entry.path()) else {
                continue;
            };
            if !path.starts_with(root) {
                out.rejected += 1;
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                out.rejected += 1;
                continue;
            };
            let modified_ms = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or_else(now_ms);
            out.candidates.push(Candidate {
                host,
                path,
                metadata,
                modified_ms,
            });
            // Keep only the newest candidates; total allocations remain bounded.
            if out.candidates.len() >= MAX_CANDIDATES * 2 {
                out.candidates
                    .sort_by_key(|c| std::cmp::Reverse(c.modified_ms));
                out.candidates.truncate(MAX_CANDIDATES);
                out.limited = true;
            }
        }
    }
    dirs.sort_by(|a, b| b.cmp(a));
    for dir in dirs {
        if *visits >= MAX_VISITS {
            out.limited = true;
            break;
        }
        scan(root, &dir, host, depth + 1, visits, out, status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dated_logs_and_explicit_roots_exclude_other_data() {
        let t = tempfile::tempdir().unwrap();
        let day = t.path().join("2026/11/12");
        fs::create_dir_all(&day).unwrap();
        fs::write(day.join("desktop.log"), "").unwrap();
        fs::write(t.path().join("auth.json"), "private").unwrap();
        fs::create_dir(t.path().join("sessions")).unwrap();
        fs::write(t.path().join("sessions/private.log"), "private").unwrap();
        let d = discover(&[Root {
            host: Host::CodexDesktop,
            path: t.path().into(),
        }]);
        assert_eq!(d.candidates.len(), 1);
        assert_eq!(d.roots[0].status, "watching");
        let d = discover(&[Root {
            host: Host::ClaudeDesktop,
            path: t.path().join("missing"),
        }]);
        assert_eq!(d.roots[0].status, "waiting for directory");
    }
    #[test]
    fn claude_rotations_are_selected_but_symlinks_are_not() {
        let t = tempfile::tempdir().unwrap();
        for n in [
            "mcp.log",
            "mcp1.log",
            "mcp-server-example.log",
            "main.log",
            "mcp.json",
        ] {
            fs::write(t.path().join(n), "").unwrap();
        }
        std::os::unix::fs::symlink(
            t.path().join("main.log"),
            t.path().join("mcp-server-link.log"),
        )
        .unwrap();
        let d = discover(&[Root {
            host: Host::ClaudeDesktop,
            path: t.path().into(),
        }]);
        assert_eq!(d.candidates.len(), 3);
        assert_eq!(d.rejected, 1);
    }
    #[test]
    fn noisy_directories_have_a_disclosed_budget() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..600 {
            fs::write(t.path().join(format!("{i}.log")), "").unwrap();
        }
        let d = discover(&[Root {
            host: Host::CodexDesktop,
            path: t.path().into(),
        }]);
        assert_eq!(d.candidates.len(), 512);
        assert!(d.limited);
    }
}

//! Private, bounded call context. Only the inspector can request the input preview.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const PREVIEW_BYTES: usize = 1536;

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize)]
pub struct Inspection {
    pub chat: Option<String>,
    pub operation: Option<String>,
    pub exit_code: Option<i32>,
    pub input_kind: Option<&'static str>,
    pub truncated: bool,
    #[serde(skip)]
    pub input: Option<Arc<str>>,
}

fn prefix(value: &str, max: usize) -> &str {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

pub fn display_name(value: &str) -> Option<String> {
    let value = prefix(value.trim(), 96);
    (!value.is_empty()
        && !value.chars().any(|c| {
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        }))
    .then(|| value.to_owned())
}

#[derive(Deserialize)]
struct Arguments {
    cmd: Option<String>,
    code: Option<String>,
}

fn shell_operation(command: &str) -> String {
    let mut words = command.split_ascii_whitespace();
    let first = words.next().unwrap_or("").rsplit('/').next().unwrap_or("");
    let verb = match first {
        "git" | "cargo" | "npm" | "gh" | "docker" | "pnpm" => {
            let sub = words.next().unwrap_or("");
            return if matches!(
                sub,
                "diff"
                    | "log"
                    | "show"
                    | "status"
                    | "add"
                    | "commit"
                    | "push"
                    | "fetch"
                    | "checkout"
                    | "switch"
                    | "rev-parse"
                    | "branch"
                    | "worktree"
                    | "test"
                    | "check"
                    | "run"
                    | "build"
                    | "clippy"
                    | "fmt"
                    | "install"
                    | "ci"
                    | "pr"
                    | "issue"
                    | "ps"
            ) {
                format!("{first} {sub}")
            } else {
                format!("Run {first}")
            };
        }
        "rg" | "grep" => "Search files",
        "cat" | "head" | "tail" | "sed" | "less" => "Read file contents",
        "ls" | "find" => "List files",
        "python" | "python3" => "Run Python",
        "node" => "Run Node.js",
        "curl" => "Make an HTTP request",
        "pwd" => "Inspect working directory",
        _ => "Run shell command",
    };
    verb.into()
}

fn script_operation(script: &str) -> String {
    // These are literal references in the submitted source, never child spans.
    let known = [
        ("tools.exec_command(", "shell"),
        ("tools.apply_patch(", "patch"),
        ("tools.write_stdin(", "terminal input"),
        ("tools.web__run(", "web"),
        ("tools.view_image(", "image"),
    ];
    let refs: Vec<_> = known
        .iter()
        .filter(|(key, _)| script.contains(key))
        .map(|(_, label)| *label)
        .collect();
    if refs.is_empty() {
        "Run tool script".into()
    } else {
        format!("Tool script referencing {}", refs.join(", "))
    }
}

impl Inspection {
    pub fn capture(tool: &str, arguments: Option<&str>, output: Option<&str>) -> Self {
        let mut result = Self::default();
        if tool == "functions.exec_command" {
            // Parse only the CLI envelope before stdout. User-controlled stdout can contain exit messages.
            result.exit_code = output.and_then(shell_exit);
        }
        let Some(raw) = arguments.filter(|v| !v.is_empty() && v.len() <= 64 * 1024) else {
            return result;
        };
        let (kind, input) = match tool {
            "functions.exec_command" => (
                "Shell command",
                serde_json::from_str::<Arguments>(raw)
                    .ok()
                    .and_then(|a| a.cmd),
            ),
            "functions.exec" => {
                let code = if raw.trim_start().starts_with('{') {
                    serde_json::from_str::<Arguments>(raw)
                        .ok()
                        .and_then(|a| a.code)
                } else if raw.trim_start().starts_with('"') {
                    serde_json::from_str::<String>(raw).ok()
                } else {
                    Some(raw.to_owned())
                };
                ("Tool script", code)
            }
            _ => return result,
        };
        if let Some(input) = input.filter(|s| !s.trim().is_empty()) {
            result.operation = Some(if kind == "Shell command" {
                shell_operation(input.trim())
            } else {
                script_operation(&input)
            });
            result.truncated = input.len() > PREVIEW_BYTES;
            result.input_kind = Some(kind);
            result.input = Some(Arc::from(prefix(&input, PREVIEW_BYTES)));
        }
        result
    }
}

fn shell_exit(output: &str) -> Option<i32> {
    let mut lines = output.lines();
    let chunk = lines.next()?.strip_prefix("Chunk ID: ")?;
    if chunk.is_empty() || !chunk.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let wall = lines
        .next()?
        .strip_prefix("Wall time: ")?
        .strip_suffix(" seconds")?;
    wall.parse::<f64>().ok()?;
    lines
        .next()?
        .strip_prefix("Process exited with code ")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_is_bounded_and_inputs_are_omitted_from_summaries() {
        let cmd = format!("git diff {}", "私".repeat(1000));
        let args = serde_json::json!({"cmd":cmd}).to_string();
        let c = Inspection::capture(
            "functions.exec_command",
            Some(&args),
            Some(
                "Chunk ID: abc\nWall time: 0.1 seconds\nProcess exited with code 7\nOutput:\nPRIVATE",
            ),
        );
        assert_eq!(c.exit_code, Some(7));
        assert_eq!(c.operation.as_deref(), Some("git diff"));
        assert!(c.truncated && c.input.unwrap().len() <= PREVIEW_BYTES);
        assert!(shell_exit("Output:\nProcess exited with code 7").is_none());
        let c = Inspection::capture(
            "functions.exec",
            Some("await tools.exec_command({cmd:'PRIVATE'})"),
            None,
        );
        assert_eq!(
            c.operation.as_deref(),
            Some("Tool script referencing shell")
        );
        assert!(!serde_json::to_string(&c).unwrap().contains("PRIVATE"));
        assert!(
            Inspection::capture("other.tool", Some(&args), None)
                .input
                .is_none()
        );
    }
}

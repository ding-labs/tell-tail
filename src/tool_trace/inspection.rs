//! Private, bounded call context. Only the inspector can request the input preview.
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;

const PREVIEW_BYTES: usize = 1536;

#[derive(Clone, Default, Debug, Eq, PartialEq, Serialize)]
pub struct Inspection {
    #[serde(skip)]
    pub locator: Option<super::turn::Locator>,
    pub chat: Option<String>,
    pub operation: Option<String>,
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<&'static str>,
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

fn basename(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()?
        .to_str()
        .and_then(display_name)
}

// Only known parameter names can reach the UI. Arbitrary map keys may contain user content.
const PARAMETERS: &[&str] = &[
    "owner",
    "repo",
    "repository",
    "path",
    "branch",
    "ref",
    "query",
    "q",
    "url",
    "document_id",
    "file_id",
    "project_id",
    "account_id",
    "issue_number",
    "pull_number",
    "method",
    "limit",
    "code",
    "cmd",
    "workdir",
    "session_id",
    "search_query",
    "open",
    "find",
    "click",
];

fn patch_operation(raw: &str) -> Option<String> {
    if raw.lines().next()? != "*** Begin Patch" {
        return None;
    }
    let mut targets = raw.lines().filter_map(|line| {
        ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
    });
    let first = basename(targets.next()?)?;
    let others = targets.count();
    Some(if others == 0 {
        format!("Patch {first}")
    } else {
        format!("Patch {first} and {others} more")
    })
}

pub(super) fn shell_operation(command: &str) -> String {
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
        if matches!(tool, "functions.exec_command" | "functions.write_stdin") {
            result.exit_code = output.and_then(shell_exit);
        }
        let Some(raw) = arguments.filter(|v| !v.is_empty() && v.len() <= 64 * 1024) else {
            return result;
        };
        let parsed = serde_json::from_str::<Value>(raw).ok();
        let value = |key: &str| parsed.as_ref()?.get(key)?.as_str();
        if let Some(fields) = parsed.as_ref().and_then(Value::as_object) {
            result.parameters = PARAMETERS
                .iter()
                .copied()
                .filter(|key| fields.contains_key(*key))
                .take(8)
                .collect();
        }
        let (kind, input) = match tool {
            "functions.exec_command" => {
                result.directory = value("workdir").and_then(basename);
                ("Shell command", value("cmd"))
            }
            "functions.exec" => {
                let code = match parsed.as_ref() {
                    Some(Value::Object(_)) => value("code"),
                    Some(Value::String(s)) => Some(s.as_str()),
                    _ => Some(raw),
                };
                ("Tool script", code)
            }
            _ => {
                result.operation = match tool {
                    "functions.write_stdin" => value("chars").map(|s| {
                        if s.is_empty() {
                            "Poll terminal session".into()
                        } else {
                            "Send terminal input".into()
                        }
                    }),
                    "functions.view_image" => value("path")
                        .and_then(basename)
                        .map(|f| format!("View {f}")),
                    "functions.apply_patch" => {
                        patch_operation(parsed.as_ref().and_then(Value::as_str).unwrap_or(raw))
                    }
                    "mcp__cua_repl.js" | "mcp__node_repl.js" => {
                        value("title").and_then(display_name)
                    }
                    "web.run" => {
                        let labels: Vec<_> = [
                            ("search_query", "Search"),
                            ("open", "Open"),
                            ("find", "Find text"),
                            ("click", "Follow link"),
                            ("image_query", "Image search"),
                        ]
                        .into_iter()
                        .filter(|(key, _)| parsed.as_ref().and_then(|v| v.get(key)).is_some())
                        .map(|(_, label)| label)
                        .collect();
                        (!labels.is_empty()).then(|| format!("Web · {}", labels.join(", ")))
                    }
                    _ => None,
                };
                return result;
            }
        };
        if let Some(input) = input.filter(|s| !s.trim().is_empty()) {
            result.operation = Some(if kind == "Shell command" {
                shell_operation(input.trim())
            } else {
                script_operation(input)
            });
            result.truncated = input.len() > PREVIEW_BYTES;
            result.input_kind = Some(kind);
            result.input = Some(Arc::from(prefix(input, PREVIEW_BYTES)));
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
    fn other_tools_report_recognizable_operations_without_retaining_payloads() {
        let cases = [
            (
                "functions.view_image",
                r#"{"path":"/private/work/screen.png"}"#,
                "View screen.png",
            ),
            (
                "functions.write_stdin",
                r#"{"session_id":42,"chars":""}"#,
                "Poll terminal session",
            ),
            (
                "functions.write_stdin",
                r#"{"session_id":42,"chars":"SECRET"}"#,
                "Send terminal input",
            ),
            (
                "functions.apply_patch",
                "*** Begin Patch\n*** Update File: src/app.rs\n+SECRET\n*** End Patch",
                "Patch app.rs",
            ),
            (
                "mcp__cua_repl.js",
                r#"{"title":"Check app layout","code":"SECRET"}"#,
                "Check app layout",
            ),
            (
                "web.run",
                r#"{"search_query":[{"q":"SECRET"}]}"#,
                "Web · Search",
            ),
        ];
        for (tool, args, expected) in cases {
            let c = Inspection::capture(tool, Some(args), None);
            assert_eq!(c.operation.as_deref(), Some(expected));
            assert!(c.input.is_none());
            assert!(!serde_json::to_string(&c).unwrap().contains("SECRET"));
        }
        let c = Inspection::capture(
            "mcp__codex_apps__github.lookup",
            Some(r#"{"repo":"SECRET","SECRET":"SECRET"}"#),
            None,
        );
        assert_eq!(c.parameters, vec!["repo"]);
        assert!(!serde_json::to_string(&c).unwrap().contains("SECRET"));
    }

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

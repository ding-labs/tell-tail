//! Read a single local chat only when its tool inspector requests turn evidence.
pub mod links;
mod rollout;
pub use rollout::read;
const MAX_TOOLS: usize = 128;

use super::identity::Identity;
use serde::Serialize;
use std::sync::LazyLock;

static KEY: LazyLock<Identity> = LazyLock::new(Identity::default);

pub(crate) fn tag(value: &str) -> String {
    KEY.label("native", value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locator {
    pub chat: String,
    pub call: String,
    day: time::Date,
}
impl Locator {
    pub fn new(chat: &str, call: &str) -> Option<Self> {
        if chat.len() != 36
            || call.is_empty()
            || call.len() > 256
            || !chat.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
            || chat.as_bytes()[14] != b'7'
        {
            return None;
        }
        let ms = i64::from_str_radix(&format!("{}{}", &chat[..8], &chat[9..13]), 16).ok()?;
        let day = time::OffsetDateTime::from_unix_timestamp(ms / 1000)
            .ok()?
            .date();
        Some(Self {
            chat: tag(chat),
            call: tag(call),
            day,
        })
    }
}

#[derive(Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub operation: Option<String>,
    pub outcome: &'static str,
    pub exit_code: Option<i32>,
    pub selected: bool,
}

impl Tool {
    pub fn from_call(call: &super::model::Call) -> Self {
        use super::model::Status;
        let context = call.inspection.as_deref();
        let exit_code = context.and_then(|c| c.exit_code);
        let outcome = match call.status {
            Status::Conflict => "conflict",
            Status::Failed => "failed",
            Status::Denied => "denied",
            Status::Cancelled => "interrupted",
            Status::Incomplete => "incomplete",
            Status::Running => "running",
            Status::Completed if call.tool_error == Some(true) => "failed",
            Status::Completed if exit_code.is_some_and(|c| c != 0) => "nonzero_exit",
            Status::Completed => "completed",
        };
        let operation = context.and_then(|ctx| {
            if call.tool == "functions.exec_command" {
                ctx.input
                    .as_deref()
                    .map(|cmd| bounded(cmd, 200))
                    .or_else(|| ctx.operation.clone())
            } else {
                ctx.operation.clone()
            }
        });
        Self {
            name: call.tool.clone(),
            operation,
            outcome,
            exit_code,
            selected: false,
        }
    }

    pub fn failed(&self) -> bool {
        matches!(
            self.outcome,
            "failed" | "nonzero_exit" | "denied" | "interrupted" | "conflict"
        )
    }
}

#[derive(Default, Serialize)]
pub struct Evidence {
    pub matched: bool,
    pub requests: Vec<String>,
    pub requested_ms: Option<i64>,
    pub tools: Vec<Tool>,
    pub children: Vec<Tool>,
    pub children_complete: Option<bool>,
    pub partial: bool,
}

pub(super) fn bounded(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit.saturating_sub(3);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[cfg(test)]
mod tests;

//! Read a single local chat only when its tool inspector requests turn evidence.
mod rollout;
pub use rollout::read;

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

#[derive(Default, Serialize)]
pub struct Evidence {
    pub matched: bool,
    pub requests: Vec<String>,
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

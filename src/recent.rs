//! Small, in-memory error previews for interactive dashboards.
use crate::parse::Level;
use std::{collections::VecDeque, sync::Arc, time::Duration};

pub const MAX_RECENT_ERRORS: usize = 20;
pub const MAX_PREVIEW_BYTES: usize = 512;

#[derive(Clone, Debug)]
pub struct RecentError {
    pub record: u64,
    pub elapsed_seconds: f64,
    pub level: Level,
    pub preview: Arc<str>,
    pub truncated: bool,
}

#[derive(Default)]
pub(crate) struct RecentErrors(VecDeque<RecentError>);

impl RecentErrors {
    pub(crate) fn record(&mut self, level: Level, record: u64, at: Duration, line: &[u8]) {
        if !matches!(level, Level::Error | Level::Fatal) {
            return;
        }
        let prefix = &line[..line.len().min(MAX_PREVIEW_BYTES)];
        let mut preview = String::with_capacity(MAX_PREVIEW_BYTES);
        let mut truncated = prefix.len() < line.len();
        for ch in String::from_utf8_lossy(prefix).chars() {
            // Keep previews on one line and prevent terminal/control spoofing.
            let ch = if ch.is_control()
                || matches!(ch, '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                ' '
            } else {
                ch
            };
            if preview.len() + ch.len_utf8() > MAX_PREVIEW_BYTES {
                truncated = true;
                break;
            }
            preview.push(ch);
        }
        if self.0.len() == MAX_RECENT_ERRORS {
            self.0.pop_front();
        }
        self.0.push_back(RecentError {
            record,
            elapsed_seconds: at.as_secs_f64(),
            level,
            preview: preview.into(),
            truncated,
        });
    }

    pub(crate) fn snapshot(&self) -> Vec<RecentError> {
        self.0.iter().rev().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_bounded_newest_first_and_safe_for_terminals() {
        let mut recent = RecentErrors::default();
        recent.record(Level::Warn, 1, Duration::ZERO, b"WARN ignored");
        assert!(recent.snapshot().is_empty());
        for record in 2..=100 {
            recent.record(Level::Error, record, Duration::ZERO, b"ERROR example");
        }
        let before = recent.snapshot();
        recent.record(
            Level::Fatal,
            101,
            Duration::from_millis(1500),
            format!("FATAL \x1b[2J\r\t\u{202e}{}", "é".repeat(512)).as_bytes(),
        );
        let entries = recent.snapshot();
        assert_eq!(entries.len(), MAX_RECENT_ERRORS);
        assert_eq!(entries.first().unwrap().record, 101);
        assert_eq!(entries.last().unwrap().record, 82);
        let latest = &entries[0];
        assert_eq!(latest.elapsed_seconds, 1.5);
        assert!(latest.truncated);
        assert!(latest.preview.len() <= MAX_PREVIEW_BYTES);
        assert!(!latest.preview.chars().any(char::is_control));
        assert!(!latest.preview.contains('\u{202e}'));
        assert_eq!(before[0].record, 100, "frozen previews stay unchanged");
        recent.record(Level::Error, 102, Duration::from_secs(2), b"ERROR \xff");
        assert!(recent.snapshot()[0].preview.contains('\u{fffd}'));
    }
}

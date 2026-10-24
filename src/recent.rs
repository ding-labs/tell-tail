//! Small, in-memory error previews for interactive dashboards.
use crate::parse::Level;
use std::{sync::Arc, time::Duration};

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

struct BufferedError {
    record: u64,
    at: Duration,
    level: Level,
    bytes: [u8; MAX_PREVIEW_BYTES],
    len: usize,
    truncated: bool,
}

pub(crate) struct RecentErrors {
    entries: Vec<BufferedError>,
    next: usize,
}

impl Default for RecentErrors {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(MAX_RECENT_ERRORS),
            next: 0,
        }
    }
}

impl RecentErrors {
    pub(crate) fn record(&mut self, level: Level, record: u64, at: Duration, line: &[u8]) {
        if !matches!(level, Level::Error | Level::Fatal) {
            return;
        }
        let prefix = &line[..line.len().min(MAX_PREVIEW_BYTES)];
        if self.entries.len() < MAX_RECENT_ERRORS {
            self.entries.push(BufferedError {
                record,
                at,
                level,
                bytes: [0; MAX_PREVIEW_BYTES],
                len: 0,
                truncated: false,
            });
        }
        let entry = &mut self.entries[self.next];
        entry.record = record;
        entry.at = at;
        entry.level = level;
        entry.bytes[..prefix.len()].copy_from_slice(prefix);
        entry.len = prefix.len();
        entry.truncated = prefix.len() < line.len();
        self.next = (self.next + 1) % MAX_RECENT_ERRORS;
    }

    pub(crate) fn snapshot(&self) -> Vec<RecentError> {
        (0..self.entries.len())
            .map(|offset| {
                let index = (self.next + self.entries.len() - 1 - offset) % self.entries.len();
                self.entries[index].snapshot()
            })
            .collect()
    }
}

impl BufferedError {
    fn snapshot(&self) -> RecentError {
        // Decode only the retained sample at publication time. Ingestion reuses
        // fixed byte slots instead of allocating and sanitizing every error.
        let mut preview = String::with_capacity(MAX_PREVIEW_BYTES);
        let mut truncated = self.truncated;
        for ch in String::from_utf8_lossy(&self.bytes[..self.len]).chars() {
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
        RecentError {
            record: self.record,
            elapsed_seconds: self.at.as_secs_f64(),
            level: self.level,
            preview: preview.into(),
            truncated,
        }
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

    #[test]
    fn frozen_previews_survive_reuse_and_shorter_records() {
        let mut recent = RecentErrors::default();
        for record in 1..=20 {
            recent.record(
                Level::Error,
                record,
                Duration::ZERO,
                format!("ERROR original {record}").as_bytes(),
            );
        }
        let frozen = recent.snapshot();
        for record in 21..=40 {
            recent.record(Level::Fatal, record, Duration::from_secs(1), b"short");
        }
        for (offset, entry) in frozen.iter().enumerate() {
            assert_eq!(
                entry.preview.as_ref(),
                format!("ERROR original {}", 20 - offset)
            );
        }
        for (offset, entry) in recent.snapshot().iter().enumerate() {
            assert_eq!(entry.record, 40 - offset as u64);
            assert_eq!(entry.preview.as_ref(), "short");
            assert!(!entry.truncated);
            assert_eq!(entry.elapsed_seconds, 1.0);
        }
    }

    #[test]
    fn invalid_utf8_expansion_and_cut_codepoints_stay_bounded() {
        let mut recent = RecentErrors::default();
        recent.record(Level::Error, 1, Duration::ZERO, &[0xff; MAX_PREVIEW_BYTES]);
        let entry = recent.snapshot().remove(0);
        assert!(entry.truncated);
        assert!(entry.preview.len() <= MAX_PREVIEW_BYTES);
        assert!(entry.preview.chars().all(|ch| ch == '\u{fffd}'));
        let input = "\u{202e}é".repeat(MAX_PREVIEW_BYTES);
        recent.record(Level::Fatal, 2, Duration::ZERO, input.as_bytes());
        let entry = recent.snapshot().remove(0);
        assert!(entry.truncated);
        assert!(entry.preview.len() <= MAX_PREVIEW_BYTES);
        assert!(!entry.preview.contains('\u{202e}'));
        assert!(!entry.preview.chars().any(char::is_control));
    }
}

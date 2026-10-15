//! A single rolling-window error-percentage rule with bounded session state.
use crate::{parse::Level, stats::Counts};
use std::time::Duration;

pub struct Rule {
    threshold_percent: f64,
    min_records: u64,
    cooldown: Duration,
    latched: bool,
    last_sent: Option<Duration>,
}

impl Rule {
    pub fn new(threshold_percent: f64, min_records: u64, cooldown: Duration) -> Self {
        assert!(threshold_percent.is_finite() && (0.0..=100.0).contains(&threshold_percent));
        assert!(min_records > 0);
        Self {
            threshold_percent,
            min_records,
            cooldown,
            latched: false,
            last_sent: None,
        }
    }

    /// Insufficient samples leave the latch unchanged. Rearming requires an
    /// eligible window below the threshold; idle time is not evidence of recovery.
    /// A breach during cooldown can fire later if it remains above the threshold.
    pub fn should_fire(&mut self, counts: &Counts, at: Duration) -> bool {
        if counts.records < self.min_records {
            return false;
        }
        if error_percent(counts) < self.threshold_percent {
            self.latched = false;
            return false;
        }
        !self.latched
            && self
                .last_sent
                .is_none_or(|last| at.saturating_sub(last) >= self.cooldown)
    }

    /// Commit only after the bounded delivery queue accepts the event. Delivery
    /// failure still consumes the breach; retries would require receiver deduping.
    pub fn mark_sent(&mut self, at: Duration) {
        self.latched = true;
        self.last_sent = Some(at);
    }
}

pub fn error_percent(counts: &Counts) -> f64 {
    let errors = counts.levels.get(Level::Error) + counts.levels.get(Level::Fatal);
    if counts.records == 0 {
        0.0
    } else {
        errors as f64 / counts.records as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::Levels;

    fn counts(errors: u64, fatal: u64, info: u64, unknown: u64) -> Counts {
        Counts {
            records: errors + fatal + info + unknown,
            levels: Levels([0, 0, info, 0, errors, fatal, unknown]),
            ..Counts::default()
        }
    }

    #[test]
    fn inclusive_threshold_counts_fatal_and_unknown_denominator() {
        let mut rule = Rule::new(25.0, 100, Duration::from_secs(300));
        assert!(!rule.should_fire(&counts(24, 0, 26, 50), Duration::ZERO));
        assert!(rule.should_fire(&counts(24, 1, 25, 50), Duration::ZERO));
        assert_eq!(error_percent(&counts(24, 1, 25, 50)), 25.0);
    }

    #[test]
    fn minimum_sample_and_empty_windows_never_fire() {
        let mut rule = Rule::new(0.0, 100, Duration::ZERO);
        assert!(!rule.should_fire(&Counts::default(), Duration::ZERO));
        assert!(!rule.should_fire(&counts(99, 0, 0, 0), Duration::ZERO));
        assert!(rule.should_fire(&counts(0, 0, 100, 0), Duration::ZERO));
        let mut all_errors = Rule::new(100.0, 1, Duration::ZERO);
        assert!(!all_errors.should_fire(&counts(99, 0, 1, 0), Duration::ZERO));
        assert!(all_errors.should_fire(&counts(99, 1, 0, 0), Duration::ZERO));
    }

    #[test]
    fn rearm_and_cooldown_require_a_new_eligible_breach() {
        let mut rule = Rule::new(25.0, 100, Duration::from_secs(300));
        let high = counts(25, 0, 75, 0);
        let low = counts(24, 0, 76, 0);
        assert!(rule.should_fire(&high, Duration::ZERO));
        // A full queue does not consume the alert.
        assert!(rule.should_fire(&high, Duration::from_secs(1)));
        rule.mark_sent(Duration::from_secs(1));
        assert!(!rule.should_fire(&high, Duration::from_secs(500)));
        assert!(!rule.should_fire(&Counts::default(), Duration::from_secs(501)));
        assert!(!rule.should_fire(&high, Duration::from_secs(502)));
        assert!(!rule.should_fire(&low, Duration::from_secs(503)));
        assert!(rule.should_fire(&high, Duration::from_secs(504)));
        rule.mark_sent(Duration::from_secs(504));
        assert!(!rule.should_fire(&low, Duration::from_secs(505)));
        assert!(!rule.should_fire(&high, Duration::from_secs(506)));
        assert!(!rule.should_fire(&high, Duration::from_secs(803)));
        assert!(rule.should_fire(&high, Duration::from_secs(804)));
    }
}

//! A single rolling-window error-percentage rule with bounded session state.
use crate::{
    cli::Args,
    parse::Level,
    stats::{Counts, Snapshot, State},
    webhook_delivery::DeliveryQueue,
};
use std::{io, time::Duration};

pub struct Alerts {
    rule: Rule,
    threshold_percent: f64,
    message: String,
    delivery: DeliveryQueue,
}

impl Alerts {
    pub fn start(args: &Args) -> io::Result<Option<Self>> {
        let (Some(url), Some(threshold_percent)) = (&args.alert_webhook, args.alert_error_percent)
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            rule: Rule::new(
                threshold_percent,
                args.alert_min_records,
                Duration::from_secs(u64::from(args.alert_cooldown)),
            ),
            threshold_percent,
            message: args
                .alert_message
                .clone()
                .unwrap_or_else(|| "Tell-Tail error percentage reached its threshold.".into()),
            delivery: DeliveryQueue::start(url.clone(), std::env::current_exe()?)?,
        }))
    }

    pub fn observe(&mut self, snapshot: &Snapshot) {
        if matches!(snapshot.state, State::Stopped | State::Failed) {
            return;
        }
        let at = Duration::from_secs_f64(snapshot.elapsed_seconds);
        if let Some(event) = self.rule.next_event(&snapshot.window.counts, at) {
            if event == AlertEvent::Recovery || self.delivery.try_send(self.payload(snapshot)) {
                self.rule.commit(event, at);
            }
        }
    }

    fn payload(&self, snapshot: &Snapshot) -> String {
        let counts = &snapshot.window.counts;
        serde_json::json!({
            "schema_version": 1,
            "event": "error_threshold_met",
            "message": self.message,
            "session_id": snapshot.session_id,
            "source": snapshot.source,
            "observed_at_unix_seconds": snapshot.observed_at_unix_seconds,
            "window_seconds": snapshot.window.configured_seconds,
            "observed_seconds": snapshot.window.observed_seconds,
            "records": counts.records,
            "error_records": counts.levels.get(Level::Error) + counts.levels.get(Level::Fatal),
            "error_percent": error_percent(counts),
            "threshold_percent": self.threshold_percent,
        })
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertEvent {
    Breach,
    Recovery,
}

pub struct Rule {
    threshold_percent: f64,
    min_records: u64,
    cooldown: Duration,
    latched: bool,
    last_breach: Option<Duration>,
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
            last_breach: None,
        }
    }

    /// Insufficient samples do not change state; idle time is not recovery.
    /// Recovery bypasses cooldown. A breach during cooldown can fire later if
    /// it remains above the threshold. Evaluation never consumes an event.
    pub fn next_event(&self, counts: &Counts, at: Duration) -> Option<AlertEvent> {
        if counts.records < self.min_records {
            return None;
        }
        if error_percent(counts) < self.threshold_percent {
            return self.latched.then_some(AlertEvent::Recovery);
        }
        (!self.latched
            && self
                .last_breach
                .is_none_or(|last| at.saturating_sub(last) >= self.cooldown))
        .then_some(AlertEvent::Breach)
    }

    /// Apply an accepted transition. Only breaches advance the cooldown clock.
    pub fn commit(&mut self, event: AlertEvent, at: Duration) {
        self.latched = event == AlertEvent::Breach;
        if self.latched {
            self.last_breach = Some(at);
        }
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
        let rule = Rule::new(25.0, 100, Duration::from_secs(300));
        assert_eq!(
            rule.next_event(&counts(24, 0, 26, 50), Duration::ZERO),
            None
        );
        assert_eq!(
            rule.next_event(&counts(24, 1, 25, 50), Duration::ZERO),
            Some(AlertEvent::Breach)
        );
        assert_eq!(error_percent(&counts(24, 1, 25, 50)), 25.0);
    }

    #[test]
    fn minimum_sample_and_empty_windows_never_fire() {
        let mut rule = Rule::new(0.0, 100, Duration::ZERO);
        assert_eq!(rule.next_event(&Counts::default(), Duration::ZERO), None);
        assert_eq!(rule.next_event(&counts(99, 0, 0, 0), Duration::ZERO), None);
        assert_eq!(
            rule.next_event(&counts(0, 0, 100, 0), Duration::ZERO),
            Some(AlertEvent::Breach)
        );
        rule.commit(AlertEvent::Breach, Duration::ZERO);
        // An inclusive zero threshold cannot recover, even at zero errors.
        assert_eq!(rule.next_event(&counts(0, 0, 100, 0), Duration::ZERO), None);
        let all_errors = Rule::new(100.0, 1, Duration::ZERO);
        assert_eq!(
            all_errors.next_event(&counts(99, 0, 1, 0), Duration::ZERO),
            None
        );
        assert_eq!(
            all_errors.next_event(&counts(99, 1, 0, 0), Duration::ZERO),
            Some(AlertEvent::Breach)
        );
    }

    #[test]
    fn rearm_and_cooldown_require_a_new_eligible_breach() {
        let mut rule = Rule::new(25.0, 100, Duration::from_secs(300));
        let high = counts(25, 0, 75, 0);
        let low = counts(24, 0, 76, 0);
        assert_eq!(
            rule.next_event(&high, Duration::ZERO),
            Some(AlertEvent::Breach)
        );
        // A full queue does not consume either kind of event.
        assert_eq!(
            rule.next_event(&high, Duration::from_secs(1)),
            Some(AlertEvent::Breach)
        );
        rule.commit(AlertEvent::Breach, Duration::from_secs(1));
        assert_eq!(rule.next_event(&high, Duration::from_secs(500)), None);
        assert_eq!(
            rule.next_event(&Counts::default(), Duration::from_secs(501)),
            None
        );
        assert_eq!(rule.next_event(&high, Duration::from_secs(502)), None);
        assert_eq!(
            rule.next_event(&low, Duration::from_secs(503)),
            Some(AlertEvent::Recovery)
        );
        assert_eq!(
            rule.next_event(&low, Duration::from_secs(504)),
            Some(AlertEvent::Recovery)
        );
        rule.commit(AlertEvent::Recovery, Duration::from_secs(504));
        assert_eq!(rule.next_event(&low, Duration::from_secs(504)), None);
        assert_eq!(
            rule.next_event(&high, Duration::from_secs(504)),
            Some(AlertEvent::Breach)
        );
        rule.commit(AlertEvent::Breach, Duration::from_secs(504));
        assert_eq!(
            rule.next_event(&low, Duration::from_secs(505)),
            Some(AlertEvent::Recovery)
        );
        rule.commit(AlertEvent::Recovery, Duration::from_secs(505));
        assert_eq!(rule.next_event(&high, Duration::from_secs(506)), None);
        assert_eq!(rule.next_event(&high, Duration::from_secs(803)), None);
        assert_eq!(
            rule.next_event(&high, Duration::from_secs(804)),
            Some(AlertEvent::Breach)
        );
    }

    #[test]
    fn recovery_requires_a_prior_breach_and_enough_records_strictly_below_threshold() {
        let mut rule = Rule::new(25.0, 100, Duration::from_secs(300));
        let low = counts(0, 0, 100, 0);
        assert_eq!(rule.next_event(&low, Duration::ZERO), None);
        rule.commit(AlertEvent::Breach, Duration::ZERO);
        for counts in [Counts::default(), counts(0, 0, 99, 0), counts(25, 0, 75, 0)] {
            assert_eq!(rule.next_event(&counts, Duration::from_secs(1)), None);
        }
        assert_eq!(
            rule.next_event(&low, Duration::from_secs(1)),
            Some(AlertEvent::Recovery)
        );
        rule.commit(AlertEvent::Recovery, Duration::from_secs(1));
        assert_eq!(rule.next_event(&low, Duration::from_secs(2)), None);
    }
}

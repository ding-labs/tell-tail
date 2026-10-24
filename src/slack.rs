//! Slack incoming-webhook messages, using the existing bounded HTTP delivery.
use crate::{
    alerts::{AlertEvent, error_percent},
    parse::Level,
    stats::Snapshot,
};

pub fn payload(
    snapshot: &Snapshot,
    event: AlertEvent,
    message: &str,
    threshold_percent: f64,
) -> String {
    let title = match event {
        AlertEvent::Breach => "🚨 Tell-Tail alert: error threshold reached",
        AlertEvent::Recovery => "✅ Tell-Tail recovery: error rate below threshold",
    };
    let counts = &snapshot.window.counts;
    // Bound unusually long paths without cutting a UTF-8 character. The full
    // 4096-byte custom message still fits comfortably within Slack's text limit.
    let mut source: String = snapshot.source.chars().take(1024).collect();
    if source.len() < snapshot.source.len() {
        source.push('…');
    }
    let text = format!(
        "{title}\n{message}\n\
         Errors + fatal: {:.2}% ({} / {} records); threshold: {threshold_percent}%.\n\
         Window: {}s ({:.1}s observed).\nSource: {source}\nSession: {}",
        error_percent(counts),
        counts.levels.get(Level::Error) + counts.levels.get(Level::Fatal),
        counts.records,
        snapshot.window.configured_seconds,
        snapshot.window.observed_seconds,
        snapshot.session_id,
    );
    // Escape Slack control characters so paths/messages cannot become mentions
    // or links. Preserve literal message text and suppress link previews.
    let text = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    serde_json::json!({
        "text": text,
        "mrkdwn": false,
        "unfurl_links": false,
        "unfurl_media": false,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{Levels, State, Stats};
    use std::time::Duration;

    #[test]
    fn messages_distinguish_transitions_and_include_window_statistics() {
        let mut snapshot = Stats::new(60, "app<&>.log".into()).snapshot(
            Duration::from_secs(5),
            State::Following,
            0,
            None,
        );
        snapshot.window.counts.records = 100;
        snapshot.window.counts.levels = Levels([0, 0, 75, 0, 20, 5, 0]);
        for (event, title) in [
            (
                AlertEvent::Breach,
                "🚨 Tell-Tail alert: error threshold reached",
            ),
            (
                AlertEvent::Recovery,
                "✅ Tell-Tail recovery: error rate below threshold",
            ),
        ] {
            let body: serde_json::Value = serde_json::from_str(&payload(
                &snapshot,
                event,
                "Check \"service\"\n<!channel> & café",
                25.0,
            ))
            .unwrap();
            let text = body["text"].as_str().unwrap();
            assert!(text.starts_with(title));
            assert!(text.contains("Check \"service\"\n&lt;!channel&gt; &amp; café"));
            assert!(text.contains("Errors + fatal: 25.00% (25 / 100 records); threshold: 25%."));
            assert!(text.contains("Window: 60s (5.0s observed)."));
            assert!(text.contains("Source: app&lt;&amp;&gt;.log"));
            assert!(text.contains(&snapshot.session_id));
            assert_eq!(body["mrkdwn"], false);
            assert_eq!(body["unfurl_links"], false);
            assert_eq!(body["unfurl_media"], false);
        }
    }

    #[test]
    fn maximum_messages_and_long_unicode_sources_fit_delivery_limits() {
        let snapshot =
            Stats::new(3600, "🦀".repeat(1025)).snapshot(Duration::ZERO, State::Reading, 0, None);
        for message in ["<".repeat(4096), "\0".repeat(4096)] {
            let body = payload(&snapshot, AlertEvent::Breach, &message, 0.001);
            assert!(body.len() <= crate::webhook::MAX_REQUEST / 2);
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            let text = body["text"].as_str().unwrap();
            assert!(text.chars().count() < 40_000);
            assert!(text.contains(&format!("Source: {}…\n", "🦀".repeat(1024))));
            assert!(text.contains("threshold: 0.001%."));
        }
    }
}

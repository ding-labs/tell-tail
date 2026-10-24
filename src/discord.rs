//! Discord incoming-webhook embeds, using the existing bounded HTTP delivery.
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
    let (title, color) = match event {
        AlertEvent::Breach => ("🚨 Tell-Tail alert: error threshold reached", 0xdc2626),
        AlertEvent::Recovery => (
            "✅ Tell-Tail recovery: error rate below threshold",
            0x16a34a,
        ),
    };
    let counts = &snapshot.window.counts;
    let mut embed = serde_json::json!({
        "title": title,
        "color": color,
        "fields": [
            {
                "name": "Errors + fatal",
                "value": format!("{:.2}% ({} / {} records)", error_percent(counts),
                    counts.levels.get(Level::Error) + counts.levels.get(Level::Fatal), counts.records),
                "inline": true,
            },
            { "name": "Threshold", "value": format!("{threshold_percent}%"), "inline": true },
            {
                "name": "Window",
                "value": format!("{}s ({:.1}s observed)",
                    snapshot.window.configured_seconds, snapshot.window.observed_seconds),
                "inline": true,
            },
            { "name": "Source", "value": bounded_source(&snapshot.source) },
            { "name": "Session", "value": snapshot.session_id },
        ],
    });
    // The CLI's 4096-byte message limit fits Discord's 4096-character embed
    // description. Together with the bounded source, the embed stays below 6000.
    if !message.is_empty() {
        embed["description"] = message.into();
    }
    serde_json::json!({ "embeds": [embed], "allowed_mentions": { "parse": [] } }).to_string()
}

fn bounded_source(value: &str) -> String {
    // Counting UTF-16 units also covers Discord clients that count emoji as two
    // characters. Reserve one unit for the ellipsis when shortening a field.
    if value.encode_utf16().count() <= 1024 {
        return value.into();
    }
    let mut source = String::new();
    let mut units = 0;
    for ch in value.chars() {
        if units + ch.len_utf16() > 1023 {
            break;
        }
        source.push(ch);
        units += ch.len_utf16();
    }
    source.push('…');
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        stats::{Levels, State, Stats},
        webhook::{MAX_REQUEST, Request},
    };
    use std::time::Duration;

    #[test]
    fn embeds_distinguish_transitions_preserve_messages_and_disable_mentions() {
        let mut snapshot = Stats::new(60, "app<&>.log".into()).snapshot(
            Duration::from_secs(5),
            State::Following,
            0,
            None,
        );
        snapshot.window.counts.records = 100;
        snapshot.window.counts.levels = Levels([0, 0, 75, 0, 20, 5, 0]);
        let message = "Check \"service\"\n@everyone <@123> & café";
        for (event, title, color) in [
            (
                AlertEvent::Breach,
                "🚨 Tell-Tail alert: error threshold reached",
                0xdc2626,
            ),
            (
                AlertEvent::Recovery,
                "✅ Tell-Tail recovery: error rate below threshold",
                0x16a34a,
            ),
        ] {
            let body: serde_json::Value =
                serde_json::from_str(&payload(&snapshot, event, message, 25.0)).unwrap();
            assert_eq!(body["allowed_mentions"], serde_json::json!({ "parse": [] }));
            assert_eq!(body["embeds"].as_array().unwrap().len(), 1);
            let embed = &body["embeds"][0];
            assert_eq!(embed["title"], title);
            assert_eq!(embed["color"], color);
            assert_eq!(embed["description"], message);
            let fields = &embed["fields"];
            assert_eq!(fields[0]["value"], "25.00% (25 / 100 records)");
            assert_eq!(fields[1]["value"], "25%");
            assert_eq!(fields[2]["value"], "60s (5.0s observed)");
            assert_eq!(fields[3]["value"], snapshot.source);
            assert_eq!(fields[4]["value"], snapshot.session_id);
        }
        let empty: serde_json::Value =
            serde_json::from_str(&payload(&snapshot, AlertEvent::Breach, "", 25.0)).unwrap();
        assert!(empty["embeds"][0].get("description").is_none());
    }

    #[test]
    fn full_messages_and_unicode_sources_fit_discord_and_delivery_limits() {
        for source in [
            "a".repeat(1024),
            "a".repeat(1025),
            "🦀".repeat(512),
            "🦀".repeat(513),
        ] {
            let snapshot =
                Stats::new(3600, source.clone()).snapshot(Duration::ZERO, State::Reading, 0, None);
            for message in ["x".repeat(4096), "\0".repeat(4096), "🦀".repeat(1024)] {
                let body = payload(&snapshot, AlertEvent::Breach, &message, 0.001);
                assert!(body.len() <= MAX_REQUEST / 2);
                let packet = serde_json::to_vec(&Request {
                    url: format!("https://example.com/{}", "a".repeat(2000)),
                    body: body.clone(),
                })
                .unwrap();
                assert!(packet.len() <= MAX_REQUEST);
                let body: serde_json::Value = serde_json::from_str(&body).unwrap();
                let embed = &body["embeds"][0];
                assert_eq!(embed["description"], message);
                let mut total = embed["title"].as_str().unwrap().encode_utf16().count()
                    + message.encode_utf16().count();
                for field in embed["fields"].as_array().unwrap() {
                    let name = field["name"].as_str().unwrap().encode_utf16().count();
                    let value = field["value"].as_str().unwrap().encode_utf16().count();
                    assert!(name <= 256 && value <= 1024);
                    total += name + value;
                }
                assert!(total <= 6000);
                let rendered = embed["fields"][3]["value"].as_str().unwrap();
                if source.encode_utf16().count() <= 1024 {
                    assert_eq!(rendered, source);
                } else {
                    assert!(rendered.ends_with('…'));
                    assert!(source.starts_with(rendered.trim_end_matches('…')));
                }
            }
        }
    }
}

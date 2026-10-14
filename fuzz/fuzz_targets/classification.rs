#![no_main]
use libfuzzer_sys::fuzz_target;
use tell_tail::{
    cli::Format,
    parse::{Classifier, Level},
};

fuzz_target!(|data: &[u8]| {
    let result = Classifier::new(Format::Json, "level".into()).classify(data);
    if result.parse_failure {
        assert_eq!(result.level, Level::Unknown);
    }
    // serde Value is an independent decoded-string oracle. Its numeric range
    // and recursion limits are stricter than syntax-only ignored values.
    match serde_json::from_slice::<serde_json::Value>(data.trim_ascii_start()) {
        Ok(value) if !value.is_object() => assert!(result.parse_failure),
        Ok(value) if !result.parse_failure => {
            let expected = value
                .get("level")
                .and_then(|v| v.as_str())
                .map(|s| Level::from_bytes(s.as_bytes()))
                .unwrap_or(Level::Unknown);
            assert_eq!(result.level, expected);
        }
        Err(error) => {
            let message = error.to_string();
            if !message.starts_with("number out of range")
                && !message.starts_with("recursion limit exceeded")
            {
                assert!(result.parse_failure, "accepted invalid JSON: {message}");
            }
        }
        _ => (), // Valid JSON can still contain duplicate selected fields.
    }
    let text = Classifier::new(Format::Text, "level".into()).classify(data);
    assert!(!text.parse_failure);
    let auto = Classifier::new(Format::Auto, "level".into()).classify(data);
    assert_eq!(
        auto,
        if data.trim_ascii_start().first() == Some(&b'{') {
            result
        } else {
            text
        }
    );
});

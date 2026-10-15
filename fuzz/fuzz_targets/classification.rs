#![no_main]
use libfuzzer_sys::fuzz_target;
use tell_tail::{
    cli::Format,
    parse::{Classifier, Level},
};
use tell_tail_fuzz::Json;

fuzz_target!(|data: &[u8]| {
    let result = Classifier::new(Format::Json, "level".into()).classify(data);
    if result.parse_failure {
        assert_eq!(result.level, Level::Unknown);
    }
    // Fully decode ignored fields too. The reference's numeric range and
    // recursion limits are stricter than syntax-only ignored values.
    match serde_json::from_slice::<Json>(data.trim_ascii_start()) {
        Ok(Json::Object { level }) if !result.parse_failure => {
            let expected = level
                .as_deref()
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
        Ok(Json::Object { .. }) => (), // Duplicate selected fields are rejected.
        Ok(_) => assert!(result.parse_failure),
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

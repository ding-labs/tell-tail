#![no_main]
use clap::Parser;
use libfuzzer_sys::fuzz_target;
use tell_tail::{
    cli::{Args, Format},
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
    if data.trim_ascii_start().first() == Some(&b'{') {
        assert_eq!(auto, result);
    } else {
        let logfmt = Classifier::new(Format::Logfmt, "level".into()).classify(data);
        let syslog = Classifier::new(Format::Syslog, "level".into()).classify(data);
        assert!([text, logfmt, syslog].contains(&auto));
    }
    // Exercise bounded field inference, nested traversal, numeric conventions,
    // and envelope decoding as well as the strict JSON reference above.
    thread_local! {
        static PARSERS: Vec<Classifier> = [
            vec![], vec!["--level-path", "/log/level"],
            vec!["--profile", "pino"], vec!["--profile", "python"],
            vec!["--profile", "otel"], vec!["--profile", "serilog"],
            vec!["--envelope", "docker"], vec!["--envelope", "cri"],
        ].into_iter().map(|flags| Classifier::from_args(&Args::parse_from(std::iter::once("tell-tail").chain(flags)))).collect();
    }
    PARSERS.with(|parsers| {
        for parser in parsers {
            parser.reset_envelope();
            let result = parser.classify(data);
            if result.parse_failure {
                assert_eq!(result.level, Level::Unknown);
            }
        }
    });
});

use super::{Level, Profile};

pub(super) fn text(profile: Option<Profile>, bytes: &[u8]) -> Level {
    if profile == Some(Profile::Serilog) && bytes.eq_ignore_ascii_case(b"verbose") {
        Level::Trace
    } else {
        Level::from_bytes(bytes)
    }
}
pub(super) fn number(profile: Option<Profile>, raw: &str) -> Level {
    let Some(profile) = profile else {
        return Level::Unknown;
    };
    let Ok(n) = raw.parse::<u8>() else {
        return Level::Unknown;
    };
    match (profile, n) {
        (Profile::Pino, 10) => Level::Trace,
        (Profile::Pino, 20) | (Profile::Python, 10) => Level::Debug,
        (Profile::Pino, 30) | (Profile::Python, 20) => Level::Info,
        (Profile::Pino, 40) | (Profile::Python, 30) => Level::Warn,
        (Profile::Pino, 50) | (Profile::Python, 40) => Level::Error,
        (Profile::Pino, 60) | (Profile::Python, 50) => Level::Fatal,
        (Profile::Otel, 1..=4) => Level::Trace,
        (Profile::Otel, 5..=8) => Level::Debug,
        (Profile::Otel, 9..=12) => Level::Info,
        (Profile::Otel, 13..=16) => Level::Warn,
        (Profile::Otel, 17..=20) => Level::Error,
        (Profile::Otel, 21..=24) => Level::Fatal,
        _ => Level::Unknown,
    }
}

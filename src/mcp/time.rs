//! Strict timestamp parsing for the observed UTC desktop log formats.
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

/// Unknown formats remain undated. In particular, never guess a local timezone.
pub fn timestamp_ms(s: &str) -> Option<i64> {
    if !s.is_ascii() || s.len() < 20 || s.len() > 30 || !s.ends_with('Z') {
        return None;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let number = |a: usize, z: usize| -> Option<i64> {
        let t = &s[a..z];
        t.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| t.parse().ok())
            .flatten()
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ][month as usize - 1];
    if !(1..=days).contains(&day) {
        return None;
    }
    let mut millis = 0;
    if s.len() != 20 {
        if b[19] != b'.' || s.len() < 22 {
            return None;
        }
        let fraction = &s[20..s.len() - 1];
        if !fraction.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        for (i, c) in fraction.bytes().take(3).enumerate() {
            millis += i64::from(c - b'0') * [100, 10, 1][i];
        }
    }
    let y = year - i64::from(month <= 2);
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(((days * 24 + hour) * 60 * 60 + minute * 60 + second) * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_are_validated_without_guessing_timezones() {
        assert_eq!(timestamp_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            timestamp_ms("2026-11-12T00:00:00.031Z").unwrap()
                - timestamp_ms("2026-11-11T23:59:59.999Z").unwrap(),
            32
        );
        assert_eq!(timestamp_ms("2000-02-29T00:00:00Z"), Some(951782400000));
        for s in [
            "2026-02-29T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-11-12T00:00:00+00:00",
            "2026-11-12T00:00:00.Z",
            "2026-11-12T00:00:00xZ",
        ] {
            assert_eq!(timestamp_ms(s), None, "{s}");
        }
    }
}

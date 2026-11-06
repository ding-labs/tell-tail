use super::{Classification, Level, field_index};
use serde::de::IgnoredAny;

pub(super) fn looks_like(line: &[u8]) -> bool {
    line.iter()
        .position(|b| b.is_ascii_whitespace() || *b == b'=')
        .is_some_and(|end| end > 0 && line[end] == b'=' && valid_key(&line[..end]))
}
fn valid_key(key: &[u8]) -> bool {
    !key.is_empty()
        && key
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-/@".contains(b))
}

pub(super) fn classify(line: &[u8], field: Option<&str>) -> Classification {
    match parse(line, field) {
        Ok(level) => Classification {
            level,
            parse_failure: false,
        },
        Err(()) => Classification {
            level: Level::Unknown,
            parse_failure: true,
        },
    }
}
fn parse(mut line: &[u8], field: Option<&str>) -> Result<Level, ()> {
    std::str::from_utf8(line).map_err(|_| ())?;
    let mut found = None;
    let mut seen = 0u16;
    while !line.is_empty() {
        let end = line
            .iter()
            .position(|b| b.is_ascii_whitespace() || *b == b'=')
            .ok_or(())?;
        if line[end] != b'=' || !valid_key(&line[..end]) {
            return Err(());
        }
        let key = std::str::from_utf8(&line[..end]).map_err(|_| ())?;
        line = &line[end + 1..];
        let quoted = line.first() == Some(&b'"');
        let end = if quoted {
            let mut at = 1;
            loop {
                match line.get(at) {
                    Some(b'"') => break at + 1,
                    Some(b'\\') => at += 2,
                    Some(_) => at += 1,
                    None => return Err(()),
                }
            }
        } else {
            line.iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(line.len())
        };
        let raw = &line[..end];
        if quoted {
            serde_json::from_slice::<IgnoredAny>(raw).map_err(|_| ())?;
            if !super::valid_unicode_escapes(raw) {
                return Err(());
            }
        } else if raw.contains(&b'"') || raw.contains(&b'=') {
            return Err(());
        }
        if let Some(index) = field_index(key, field) {
            if seen & (1 << index) != 0 {
                return Err(());
            }
            seen |= 1 << index;
            let level = if quoted {
                if let Ok(value) = serde_json::from_slice::<&str>(raw) {
                    Level::from_bytes(value.as_bytes())
                } else {
                    Level::from_bytes(
                        serde_json::from_slice::<String>(raw)
                            .map_err(|_| ())?
                            .as_bytes(),
                    )
                }
            } else {
                Level::from_bytes(raw)
            };
            found = Some(match found {
                Some(previous) if previous != level => Level::Unknown,
                _ => level,
            });
        }
        line = &line[end..];
        if !line.is_empty() && !line[0].is_ascii_whitespace() {
            return Err(());
        }
        line = line.trim_ascii_start();
    }
    Ok(found.unwrap_or(Level::Unknown))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_escapes_duplicates_and_malformed_suffixes_are_not_keyword_matches() {
        for (line, level, bad) in [
            (r#"msg="level=error" level=info"#, Level::Info, false),
            (
                r#"level="w\u0061rn" msg="a \"quoted\" value""#,
                Level::Warn,
                false,
            ),
            (r#"msg="ERROR""#, Level::Unknown, false),
            ("level=error severity=info", Level::Unknown, false),
            ("level=error level=error", Level::Unknown, true),
            ("level=error trailing", Level::Unknown, true),
            (r#"level=error msg="unterminated"#, Level::Unknown, true),
            (r#"level=error msg="\uD800""#, Level::Unknown, true),
            (r#"msg="safe"level=error"#, Level::Unknown, true),
        ] {
            let result = classify(line.as_bytes(), None);
            assert_eq!((result.level, result.parse_failure), (level, bad), "{line}");
        }
        assert_eq!(
            classify(b"priority=warn level=error", Some("priority")).level,
            Level::Warn
        );
        assert_eq!(
            classify(b"msg=\xff level=error", None).level,
            Level::Unknown
        );
    }
}

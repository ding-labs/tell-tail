use crate::cli::Format;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}
impl Level {
    pub const ALL: [Self; 7] = [
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warn,
        Self::Error,
        Self::Fatal,
        Self::Unknown,
    ];
    pub fn name(self) -> &'static str {
        [
            "trace", "debug", "info", "warn", "error", "fatal", "unknown",
        ][self as usize]
    }
    pub fn from_bytes(bytes: &[u8]) -> Self {
        for level in Self::ALL {
            if bytes.eq_ignore_ascii_case(level.name().as_bytes()) {
                return level;
            }
        }
        if bytes.eq_ignore_ascii_case(b"warning") {
            Self::Warn
        } else if bytes.eq_ignore_ascii_case(b"critical") || bytes.eq_ignore_ascii_case(b"panic") {
            Self::Fatal
        } else {
            Self::Unknown
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Classification {
    pub level: Level,
    pub parse_failure: bool,
}
impl Classification {
    pub fn unknown() -> Self {
        Self {
            level: Level::Unknown,
            parse_failure: false,
        }
    }
}
pub struct Classifier {
    format: Format,
    field: String,
}
impl Classifier {
    pub fn new(format: Format, field: String) -> Self {
        Self { format, field }
    }
    pub fn classify(&self, line: &[u8]) -> Classification {
        let line = trim_start(line);
        if self.format == Format::Json
            || (self.format == Format::Auto && line.first() == Some(&b'{'))
        {
            if std::str::from_utf8(line).is_err() {
                return Classification {
                    level: Level::Unknown,
                    parse_failure: true,
                };
            }
            match json_level(line, &self.field) {
                Ok(level) => Classification {
                    level,
                    parse_failure: false,
                },
                Err(_) => Classification {
                    level: Level::Unknown,
                    parse_failure: true,
                },
            }
        } else {
            Classification {
                level: text_level(line),
                parse_failure: false,
            }
        }
    }
}
fn trim_start(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii_start()
}
fn text_level(mut line: &[u8]) -> Level {
    let token_end = line
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(line.len());
    if iso_timestamp(&line[..token_end]) {
        line = trim_start(&line[token_end..]);
    }
    if let Some(bracketed) = line.strip_prefix(b"[") {
        let Some(end) = bracketed.iter().take(16).position(|&b| b == b']') else {
            return Level::Unknown;
        };
        let after = &bracketed[end + 1..];
        if !after.is_empty() && !after[0].is_ascii_whitespace() {
            return Level::Unknown;
        }
        Level::from_bytes(&bracketed[..end])
    } else {
        let end = line
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(line.len());
        Level::from_bytes(line[..end].strip_suffix(b":").unwrap_or(&line[..end]))
    }
}
fn iso_timestamp(bytes: &[u8]) -> bool {
    let b = bytes
        .strip_prefix(b"[")
        .and_then(|s| s.strip_suffix(b"]"))
        .unwrap_or(bytes);
    b.len() >= 19
        && b.len() <= 40
        && b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b'T' | b't')
        && b[13] == b':'
        && b[16] == b':'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
        && b[19..]
            .iter()
            .all(|b| b.is_ascii_digit() || b".Zz+-:".contains(b))
}
struct LevelSeed<'a>(&'a str);
impl<'de> DeserializeSeed<'de> for LevelSeed<'_> {
    type Value = Level;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Level, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for LevelSeed<'_> {
    type Value = Level;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON log object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Level, M::Error> {
        let mut found = None;
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            if key == self.0 {
                if found.is_some() {
                    return Err(de::Error::custom("duplicate severity field"));
                }
                let raw: &RawValue = map.next_value()?;
                let level = if let Ok(s) = serde_json::from_str::<&str>(raw.get()) {
                    Level::from_bytes(s.as_bytes())
                } else if let Ok(s) = serde_json::from_str::<String>(raw.get()) {
                    Level::from_bytes(s.as_bytes())
                } else {
                    Level::Unknown
                };
                found = Some(level);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(found.unwrap_or(Level::Unknown))
    }
}
fn json_level(line: &[u8], field: &str) -> Result<Level, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(line);
    let level = LevelSeed(field).deserialize(&mut de)?;
    de.end()?;
    Ok(level)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_prefixes_set_text_severity() {
        for (line, expected) in [
            ("INFO an error occurred", Level::Info),
            ("[warning] slow", Level::Warn),
            ("2026-10-14T22:01:02.123Z ERROR failed", Level::Error),
            ("[2026-10-14T22:01:02+00:00] [FATAL] failed", Level::Fatal),
            ("ERROR: failed", Level::Error),
            ("message mentions ERROR", Level::Unknown),
            ("ERRORISH hello", Level::Unknown),
            ("[WARN]ish", Level::Unknown),
            ("", Level::Unknown),
        ] {
            assert_eq!(text_level(line.as_bytes()), expected, "{line}");
        }
    }
    #[test]
    fn json_is_validated_and_field_is_top_level() {
        let c = Classifier::new(Format::Json, "level".into());
        for (s, level, bad) in [
            (r#"{"message":"ERROR", "level":"info"}"#, Level::Info, false),
            (r#"{"nested":{"level":"ERROR"}}"#, Level::Unknown, false),
            (
                r#"{"level":"e\u0072ror","other":[1,{},true]}"#,
                Level::Error,
                false,
            ),
            (r#"{"level":50}"#, Level::Unknown, false),
            (r#"{"level":"ERROR","bad":}"#, Level::Unknown, true),
            (r#"{"level":"ERROR"} trailing"#, Level::Unknown, true),
            (r#"{"level":"ERROR","level":"INFO"}"#, Level::Unknown, true),
            (r#"[]"#, Level::Unknown, true),
        ] {
            assert_eq!(
                c.classify(s.as_bytes()),
                Classification {
                    level,
                    parse_failure: bad
                },
                "{s}"
            );
        }
        assert_eq!(
            Classifier::new(Format::Auto, "severity".into())
                .classify(br#" {"severity":"WARN"}"#)
                .level,
            Level::Warn
        );
    }
    #[test]
    fn invalid_json_and_encoding_never_fall_back() {
        let c = Classifier::new(Format::Auto, "level".into());
        assert!(c.classify(b"{ ERROR hello").parse_failure);
        assert!(
            c.classify(b"{\"level\":\"ERROR\",\"msg\":\"\xff\"}")
                .parse_failure
        );
        assert_eq!(c.classify(b"INFO \xff").level, Level::Info);
    }
}

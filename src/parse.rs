mod logfmt;
mod profiles;
mod text;
use crate::cli::{Args, Format, Profile};
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt};
use text::text_level;

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
        } else if bytes.eq_ignore_ascii_case(b"information")
            || bytes.eq_ignore_ascii_case(b"informational")
        {
            Self::Info
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
    field: Option<String>,
    path: Option<Vec<String>>,
    profile: Option<Profile>,
}
impl Classifier {
    pub fn new(format: Format, field: String) -> Self {
        Self {
            format,
            field: Some(field),
            path: None,
            profile: None,
        }
    }
    pub fn from_args(args: &Args) -> Self {
        Self {
            format: args.input_format(),
            profile: args.profile,
            field: args.level_field.clone(),
            path: args
                .level_path
                .as_deref()
                .map(|path| pointer_tokens(path).unwrap_or_default()),
        }
    }
    /// Classify one record. Use classify_record when source metadata should be
    /// omitted; this compatibility API represents such metadata as unknown.
    pub fn classify(&self, line: &[u8]) -> Classification {
        if self.format == Format::AppleUnified {
            return self
                .classify_record(line)
                .unwrap_or_else(Classification::unknown);
        }
        let line = trim_start(line);
        if self.profile.is_some()
            || self.path.is_some()
            || self.format == Format::Json
            || (self.format == Format::Auto && line.first() == Some(&b'{'))
        {
            if std::str::from_utf8(line).is_err() || !valid_unicode_escapes(line) {
                return Classification {
                    level: Level::Unknown,
                    parse_failure: true,
                };
            }
            match json_selected(
                line,
                self.field
                    .as_deref()
                    .or_else(|| self.profile.map(Profile::field)),
                self.path.as_deref(),
                false,
                self.profile,
                self.profile == Some(Profile::Serilog)
                    && self.field.is_none()
                    && self.path.is_none(),
            ) {
                Ok(level) => Classification {
                    level: level.unwrap_or(Level::Unknown),
                    parse_failure: false,
                },
                Err(_) => Classification {
                    level: Level::Unknown,
                    parse_failure: true,
                },
            }
        } else if self.format == Format::Logfmt
            || (self.format == Format::Auto && logfmt::looks_like(line))
        {
            logfmt::classify(line, self.field.as_deref())
        } else {
            Classification {
                level: text_level(line),
                parse_failure: false,
            }
        }
    }
    /// Source-aware classification. None denotes Apple stream metadata, which
    /// ingestion excludes from entry counts and accounts for as skipped bytes.
    pub fn classify_record(&self, line: &[u8]) -> Option<Classification> {
        if self.format != Format::AppleUnified {
            return Some(self.classify(line));
        }
        let line = trim_start(line);
        // The native tool writes a text banner alongside its NDJSON records.
        if line.first() != Some(&b'{') {
            return None;
        }
        let bad = Classification {
            level: Level::Unknown,
            parse_failure: true,
        };
        if std::str::from_utf8(line).is_err() || !valid_unicode_escapes(line) {
            return Some(bad);
        }
        match json_level(line, Some("messageType"), true) {
            Ok(level) => level.map(|level| Classification {
                level,
                parse_failure: false,
            }),
            Err(_) => Some(bad),
        }
    }
}
// serde's IgnoredAny validates JSON syntax but skips surrogate pairing in
// ignored strings. Validate escapes without allocating or parsing numbers into
// floats; large numeric values in ignored fields remain supported.
fn valid_unicode_escapes(mut bytes: &[u8]) -> bool {
    fn hex(bytes: &[u8]) -> Option<u16> {
        bytes.get(..4)?.iter().try_fold(0, |value, &digit| {
            let digit = (digit as char).to_digit(16)? as u16;
            Some(value * 16 + digit)
        })
    }
    while let Some(index) = memchr::memchr(b'\\', bytes) {
        bytes = &bytes[index + 1..];
        if bytes.first() != Some(&b'u') {
            bytes = bytes.get(1..).unwrap_or_default();
            continue;
        }
        let Some(code) = hex(&bytes[1..]) else {
            return false;
        };
        bytes = &bytes[5..];
        if (0xD800..=0xDBFF).contains(&code) {
            let Some(low) = bytes.strip_prefix(b"\\u").and_then(hex) else {
                return false;
            };
            if !(0xDC00..=0xDFFF).contains(&low) {
                return false;
            }
            bytes = &bytes[6..];
        } else if (0xDC00..=0xDFFF).contains(&code) {
            return false;
        }
    }
    true
}
fn trim_start(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii_start()
}
pub(crate) fn pointer_tokens(path: &str) -> Option<Vec<String>> {
    if path.len() > 1024 || !path.starts_with('/') {
        return None;
    }
    let parts: Vec<_> = path[1..].split('/').collect();
    if parts.len() > 16 {
        return None;
    }
    let mut result = Vec::with_capacity(parts.len());
    for part in parts {
        let mut decoded = String::new();
        let mut chars = part.chars();
        while let Some(c) = chars.next() {
            decoded.push(if c == '~' {
                match chars.next()? {
                    '0' => '~',
                    '1' => '/',
                    _ => return None,
                }
            } else {
                c
            });
        }
        result.push(decoded);
    }
    Some(result)
}

// A fixed allowlist avoids searching message/context fields for severity words.
const LEVEL_FIELDS: &[&str] = &[
    "level",
    "severity",
    "levelname",
    "Level",
    "log.level",
    "severityText",
    "SeverityText",
];
fn field_index(key: &str, field: Option<&str>) -> Option<usize> {
    match field {
        Some(field) => (key == field).then_some(0),
        None => LEVEL_FIELDS.iter().position(|candidate| key == *candidate),
    }
}
struct LevelSeed<'a> {
    field: Option<&'a str>,
    path: Option<&'a [String]>,
    apple: bool,
    profile: Option<Profile>,
    clef_default: bool,
}
impl<'de> DeserializeSeed<'de> for LevelSeed<'_> {
    type Value = Option<Level>;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for LevelSeed<'_> {
    type Value = Option<Level>;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON log object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        let mut found = None;
        let mut seen = 0u16;
        let mut clef_seen = 0u8;
        let mut clef_valid = 0u8;
        while let Some(key) = map.next_key::<Cow<'de, str>>()? {
            let index = match self.path {
                Some(path) => path.first().filter(|field| **field == key).map(|_| 0),
                None => field_index(&key, self.field),
            };
            if let Some(index) = index {
                if seen & (1 << index) != 0 {
                    return Err(de::Error::custom("duplicate severity field"));
                }
                seen |= 1 << index;
                let raw: &RawValue = map.next_value()?;
                let value = if let Some(path) = self.path.filter(|path| path.len() > 1) {
                    Some(if raw.get().starts_with('{') {
                        json_selected(
                            raw.get().as_bytes(),
                            None,
                            Some(&path[1..]),
                            false,
                            self.profile,
                            false,
                        )
                        .map_err(de::Error::custom)?
                        .unwrap_or(Level::Unknown)
                    } else {
                        Level::Unknown
                    })
                } else if self.apple && raw.get() == "null" {
                    None
                } else {
                    let decode = |s: &str| {
                        if self.apple {
                            apple_level(s.as_bytes())
                        } else {
                            profiles::text(self.profile, s.as_bytes())
                        }
                    };
                    Some(if let Ok(s) = serde_json::from_str::<&str>(raw.get()) {
                        decode(s)
                    } else if let Ok(s) = serde_json::from_str::<String>(raw.get()) {
                        decode(&s)
                    } else {
                        profiles::number(self.profile, raw.get())
                    })
                };
                found = Some(match found {
                    Some(previous) if previous != value => Some(Level::Unknown),
                    _ => value,
                });
            } else if self.clef_default && matches!(key.as_ref(), "@t" | "@m" | "@mt") {
                let bit = match key.as_ref() {
                    "@t" => 1,
                    "@m" => 2,
                    _ => 4,
                };
                if clef_seen & bit != 0 {
                    return Err(de::Error::custom("duplicate compact event field"));
                }
                clef_seen |= bit;
                let raw: &RawValue = map.next_value()?;
                if let Ok(value) = serde_json::from_str::<String>(raw.get()) {
                    if bit != 1 || text::iso_timestamp(value.as_bytes()) {
                        clef_valid |= bit;
                    }
                }
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        if found.is_none() && self.clef_default && clef_valid & 1 != 0 && clef_valid & 6 != 0 {
            return Ok(Some(Level::Info));
        }
        Ok(found.flatten())
    }
}
fn apple_level(value: &[u8]) -> Level {
    for (name, level) in [
        (b"Default".as_slice(), Level::Info),
        (b"Info", Level::Info),
        (b"Debug", Level::Debug),
        (b"Error", Level::Error),
        (b"Fault", Level::Fatal),
    ] {
        if value.eq_ignore_ascii_case(name) {
            return level;
        }
    }
    Level::Unknown
}
fn json_level(
    line: &[u8],
    field: Option<&str>,
    apple: bool,
) -> Result<Option<Level>, serde_json::Error> {
    json_selected(line, field, None, apple, None, false)
}
fn json_selected(
    line: &[u8],
    field: Option<&str>,
    path: Option<&[String]>,
    apple: bool,
    profile: Option<Profile>,
    clef_default: bool,
) -> Result<Option<Level>, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(line);
    let level = LevelSeed {
        field,
        path,
        apple,
        profile,
        clef_default,
    }
    .deserialize(&mut de)?;
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
    #[test]
    fn validates_surrogate_pairs_even_in_ignored_fields() {
        let classifier = Classifier::new(Format::Json, "level".into());
        for escape in [
            r"\uD800",
            r"\uDC00",
            r"\uD800\u0041",
            r"\uD800\uD800",
            r"\uD8",
            r"\uD800x",
        ] {
            let line = format!(r#"{{"level":"INFO","nested":[{{"message":"{escape}"}}]}}"#);
            let result = classifier.classify(line.as_bytes());
            assert_eq!(result.level, Level::Unknown, "{line}");
            assert!(result.parse_failure, "{line}");
        }
        for escape in [
            r"\uD83D\uDE00",
            r"\\uD800",
            r#"\"quoted\""#,
            r"\u0041",
            r"\\\uD83D\uDE00",
        ] {
            let line = format!(r#"{{"level":"INFO","message":"{escape}","large":1e400}}"#);
            let result = classifier.classify(line.as_bytes());
            assert_eq!(result.level, Level::Info, "{line}");
            assert!(!result.parse_failure, "{line}");
        }
    }
}

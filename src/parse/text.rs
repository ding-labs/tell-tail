use super::Level;

// Only a small header is needed. Never allocate/copy an entire colored record.
const HEADER_BYTES: usize = 512;
pub(super) fn text_level(line: &[u8]) -> Level {
    let header = &line[..line.len().min(HEADER_BYTES)];
    if !header.contains(&0x1b) {
        return plain_level(line);
    }
    let mut clean = [0u8; HEADER_BYTES + 1];
    let mut output = 0;
    let mut input = 0;
    while input < header.len() {
        if header[input] == 0x1b {
            if header.get(input + 1) != Some(&b'[') {
                return Level::Unknown;
            }
            let start = input + 2;
            let Some(end) = header[start..].iter().take(32).position(|&b| b == b'm') else {
                return Level::Unknown;
            };
            if !header[start..start + end]
                .iter()
                .all(|b| b.is_ascii_digit() || matches!(b, b';' | b':'))
            {
                return Level::Unknown;
            }
            input = start + end + 1;
        } else {
            clean[output] = header[input];
            output += 1;
            input += 1;
        }
    }
    if line.len() > header.len() {
        // A truncated header must not turn a partial token into a complete level.
        clean[output] = b'!';
        output += 1;
    }
    plain_level(&clean[..output])
}

fn plain_level(line: &[u8]) -> Level {
    let mut line = line.trim_ascii_start();
    let mut timestamp_end = line
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(line.len());
    if line.starts_with(b"[") {
        if let Some(end) = line.iter().take(44).position(|&b| b == b']') {
            if iso_timestamp(&line[1..end])
                && line.get(end + 1).is_some_and(u8::is_ascii_whitespace)
            {
                line = line[end + 1..].trim_ascii_start();
            }
        }
    } else {
        if timestamp_end == 10 && line.get(10) == Some(&b' ') {
            timestamp_end = line[11..]
                .iter()
                .position(u8::is_ascii_whitespace)
                .map_or(line.len(), |i| i + 11);
        }
        if iso_timestamp(&line[..timestamp_end]) {
            line = line[timestamp_end..].trim_ascii_start();
        }
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
        // Python's basicConfig emits LEVEL:logger:message with no intervening space.
        let end = line
            .iter()
            .position(|b| b.is_ascii_whitespace() || *b == b':')
            .unwrap_or(line.len());
        Level::from_bytes(&line[..end])
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
        && matches!(b[10], b'T' | b't' | b' ')
        && b[13] == b':'
        && b[16] == b':'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
        && b[19..]
            .iter()
            .all(|b| b.is_ascii_digit() || b".,Zz+-:".contains(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_headers_and_colors_preserve_message_boundaries() {
        for line in [
            "WARNING:root:slow",
            "2026-11-05 12:00:00,123 WARNING slow",
            "[2026-11-05 12:00:00.123] [WARN] slow",
            "\x1b[33mWARN\x1b[0m slow",
            "\x1b[2m2026-11-05T12:00:00Z\x1b[0m \x1b[33mWARN\x1b[0m slow",
        ] {
            assert_eq!(text_level(line.as_bytes()), Level::Warn, "{line}");
        }
        for line in [
            "message:ERROR failed",
            "WARNish:message",
            "[WARN]ish",
            "2026-11-05 arbitrary ERROR",
            "\x1b]0;ERROR\x07 ready",
            "\x1b[31ERROR",
            "\x1b[2JERROR",
        ] {
            assert_eq!(text_level(line.as_bytes()), Level::Unknown, "{line}");
        }
        let long = format!("\x1b[31mERROR\x1b[0m {}", "x".repeat(10000));
        assert_eq!(text_level(long.as_bytes()), Level::Error);
        let cut = format!("\x1b[0m{}ERRORISH", " ".repeat(503));
        assert_eq!(text_level(cut.as_bytes()), Level::Unknown);
    }
}

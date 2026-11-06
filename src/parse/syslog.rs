use super::{Classification, Level};

pub(super) fn looks_like(line: &[u8]) -> bool {
    line.first() == Some(&b'<') && line.get(1).is_some_and(u8::is_ascii_digit)
}
pub(super) fn classify(line: &[u8]) -> Classification {
    match priority(line) {
        Some(priority) => Classification {
            level: match priority % 8 {
                0..=2 => Level::Fatal,
                3 => Level::Error,
                4 => Level::Warn,
                5..=6 => Level::Info,
                _ => Level::Debug,
            },
            parse_failure: false,
        },
        None => Classification {
            level: Level::Unknown,
            parse_failure: true,
        },
    }
}
fn priority(line: &[u8]) -> Option<u8> {
    let end = line.iter().take(5).position(|b| *b == b'>')?;
    let raw = line.get(1..end)?;
    if line.first() != Some(&b'<')
        || raw.is_empty()
        || !raw.iter().all(u8::is_ascii_digit)
        || (raw.len() > 1 && raw[0] == b'0')
    {
        return None;
    }
    let priority = std::str::from_utf8(raw).ok()?.parse::<u8>().ok()?;
    if priority > 191 {
        return None;
    }
    let mut body = &line[end + 1..];
    if body.starts_with(b"1 ") {
        body = &body[2..];
        for index in 0..5 {
            let end = body.iter().position(|b| *b == b' ')?;
            let token = &body[..end];
            if token.is_empty() || !token.iter().all(|b| (33..=126).contains(b)) {
                return None;
            }
            if index == 0
                && token != b"-"
                && (!super::text::iso_timestamp(token) || token.get(10) != Some(&b'T'))
            {
                return None;
            }
            if token.len() > [32, 255, 48, 128, 32][index] {
                return None;
            }
            body = &body[end + 1..];
        }
        if body == b"-" || body.starts_with(b"- ") {
            return Some(priority);
        }
        if !body.starts_with(b"[") {
            return None;
        }
        while body.starts_with(b"[") {
            body = structured_element(body)?;
        }
        if !body.is_empty() && body[0] != b' ' {
            return None;
        }
    } else {
        // Traditional RFC 3164 header: Mmm dd hh:mm:ss HOSTNAME ...
        if body.len() < 17
            || ![
                b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct",
                b"Nov", b"Dec",
            ]
            .iter()
            .any(|m| body.starts_with(*m))
        {
            return None;
        }
        if body[3] != b' '
            || body[6] != b' '
            || body[9] != b':'
            || body[12] != b':'
            || body[15] != b' '
        {
            return None;
        }
        for (start, max) in [(4, 31), (7, 23), (10, 59), (13, 59)] {
            let raw = std::str::from_utf8(&body[start..start + 2]).ok()?;
            let n = raw.trim_start().parse::<u8>().ok()?;
            if n > max || (start == 4 && n == 0) {
                return None;
            }
        }
        let host = body[16..].split(|b| *b == b' ').next()?;
        if host.is_empty() || host.len() > 255 || !host.iter().all(|b| (33..=126).contains(b)) {
            return None;
        }
    }
    Some(priority)
}
fn name(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 32
        && bytes
            .iter()
            .all(|b| (33..=126).contains(b) && !b"=]\"".contains(b))
}
fn structured_element(mut body: &[u8]) -> Option<&[u8]> {
    body = &body[1..];
    let end = body.iter().position(|b| matches!(b, b' ' | b']'))?;
    if !name(&body[..end]) {
        return None;
    }
    body = &body[end..];
    while body.first() == Some(&b' ') {
        body = &body[1..];
        let end = body.iter().position(|b| *b == b'=')?;
        if !name(&body[..end]) {
            return None;
        }
        body = body.get(end + 1..)?.strip_prefix(b"\"")?;
        let mut i = 0;
        loop {
            match body.get(i)? {
                b'"' => {
                    body = &body[i + 1..];
                    break;
                }
                b'\\' => {
                    if !matches!(body.get(i + 1)?, b'\\' | b'"' | b']') {
                        return None;
                    }
                    i += 2;
                }
                b']' => return None,
                _ => i += 1,
            }
        }
    }
    body.strip_prefix(b"]")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn priority_headers_are_required_and_message_keywords_do_not_override_them() {
        for (line, expected) in [
            (
                r#"<14>1 2026-11-05T12:00:00Z host app 123 event - ERROR in message"#,
                Level::Info,
            ),
            (
                r#"<11>1 - host app - - [example@123 a="escaped \] value"] timeout"#,
                Level::Error,
            ),
            ("<132>Nov  5 12:00:00 host app: slow", Level::Warn),
        ] {
            assert_eq!(classify(line.as_bytes()).level, expected, "{line}");
        }
        for line in [
            "<192>1 - h a - - - msg",
            "<014>1 - h a - - - msg",
            "<14>INFO ready",
            "<14>1 - h a - - [bad a=1] message",
            "Nov  5 12:00:00 host ERROR message",
        ] {
            assert!(classify(line.as_bytes()).parse_failure, "{line}");
        }
        for n in 0..=191 {
            assert!(!classify(format!("<{n}>1 - host app - - - message").as_bytes()).parse_failure);
        }
    }
}

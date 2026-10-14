# Classification corpus

These fixtures are synthetic, checked-in examples of the supported text grammar
and structured log shapes. They contain no collected application logs. Each case
specifies the expected severity of every physical record, parse-failure count,
and oversized count. Hex input represents invalid UTF-8 without relying on editor
encoding behavior.

Unsupported ANSI prefixes, logfmt, nested severity fields, and numeric profiles
are intentional unknown cases. They must not silently become severity matches
based on words appearing in a message. New format support needs an explicit
contract change and updated expectations.

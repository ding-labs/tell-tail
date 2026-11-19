import json
import unittest

from trace_probe_decode import InvalidBatch, MAX_BODY, MAX_RECORDS, decode


def vint(n):
    out = bytearray()
    while n > 127:
        out.append((n & 127) | 128)
        n >>= 7
    out.append(n)
    return bytes(out)


def message(n, value):
    return vint(n * 8 + 2) + vint(len(value)) + value


def attr(name, value):
    if isinstance(value, bool):
        raw = b"\x10" + vint(int(value))
    elif isinstance(value, int):
        raw = b"\x18" + vint(value)
    else:
        raw = message(1, value.encode())
    return message(6, message(1, name.encode()) + message(2, raw))


def batch(*records):
    return message(1, message(2, b"".join(message(2, r) for r in records)))


class DecodeTests(unittest.TestCase):
    def test_metadata_only_and_local_correlation(self):
        r = message(12, b"codex.tool_result") + b"\x09" + (42).to_bytes(8, "little")
        for k, v in {"tool_name": "mcp__codex_apps__railway_search_docs",
                     "call_id": "PRIVATE_CALL", "conversation.id": "PRIVATE_CHAT",
                     "duration_ms": 123, "success": True, "output": "PRIVATE_BALANCE",
                     "arguments": "PRIVATE_ARGS", "unknown_PRIVATE_KEY": "PRIVATE_VALUE"}.items():
            r += attr(k, v)
        r += message(5, message(1, b"PRIVATE_BODY"))
        a, b = decode(batch(r, r), b"first-run")
        self.assertEqual(a, b)
        self.assertEqual(a["tool"], "railway.search_docs")
        self.assertEqual(a["duration_ms"], 123)
        self.assertTrue(a["execution_success"])
        self.assertTrue(a["has_timestamp"])
        self.assertNotIn("PRIVATE", json.dumps(a))
        self.assertNotEqual(a["call_id_local"], decode(batch(r), b"second-run")[0]["call_id_local"])

    def test_unknown_values_never_echo(self):
        r = message(12, b"PRIVATE_EVENT") + attr("tool_name", "PRIVATE_TOOL")
        r += attr("plugin_id", "PRIVATE_PLUGIN") + attr("success", "PRIVATE_RESULT")
        r += attr("duration_ms", 2**64 - 1)
        value = decode(batch(r), b"key")[0]
        self.assertEqual(value["event"], "other")
        self.assertNotIn("PRIVATE", json.dumps(value))
        self.assertNotIn("duration_ms", value)
        self.assertNotIn("execution_success", value)

    def test_unknown_tool_fingerprints_and_string_duration(self):
        a, b, c = decode(batch(attr("tool_name", "PRIVATE_A"),
                               attr("tool_name", "PRIVATE_A"),
                               attr("tool_name", "PRIVATE_B")), b"key")
        self.assertEqual(a["tool_name_local"], b["tool_name_local"])
        self.assertNotEqual(a["tool_name_local"], c["tool_name_local"])
        self.assertNotIn("PRIVATE", json.dumps([a, b, c]))
        for value, expected in [("123", 123), ("0", 0), ("86400000", 86400000),
                                ("86400001", None), ("-1", None), ("1.5", None),
                                ("9" * 200, None), ("１２", None)]:
            result = decode(batch(attr("duration_ms", value)), b"key")[0]
            self.assertEqual(result.get("duration_ms"), expected)
        self.assertEqual(decode(batch(attr("tool_name", "exec_command")), b"key")[0]["tool"],
                         "exec_command")

    def test_tool_name_inspection_is_explicit_and_bounded(self):
        for tool in ["mcp__example__lookup", "example.lookup", "exec_command"]:
            b = batch(attr("tool_name", tool) + attr("arguments", "PRIVATE_ARGS"))
            self.assertNotIn("observed_tool_name", decode(b, b"key")[0])
            result = decode(b, b"key", inspect_tool_names=True)[0]
            self.assertEqual(result["observed_tool_name"], tool)
            self.assertNotIn("PRIVATE_ARGS", json.dumps(result))
        for tool in ["", "x" * 129, "line\nfeed", "embedded secret", "<script>", "λ"]:
            result = decode(batch(attr("tool_name", tool)), b"key", inspect_tool_names=True)[0]
            self.assertNotIn("observed_tool_name", result)

    def test_event_body_and_attribute_name(self):
        for r in [message(5, message(1, b"codex.tool_result")), attr("event.name", "codex.tool_result")]:
            self.assertEqual(decode(batch(r), b"key")[0]["event"], "codex.tool_result")

    def test_bad_and_excessive_batches(self):
        for b in [b"\x00", b"\x0a\xff", b"\x0a\x08x", b"\xff" * 11,
                  b"x" * (MAX_BODY + 1), batch(*([b""] * (MAX_RECORDS + 1)))]:
            with self.assertRaises(InvalidBatch):
                decode(b, b"key")

    def test_conflicting_and_duplicate_identity_rejected(self):
        for r in [attr("call_id", "a") + attr("call_id", "b"),
                  message(12, b"codex.tool_result") + attr("event.name", "codex.tool_decision")]:
            with self.assertRaises(InvalidBatch):
                decode(batch(r), b"key")


if __name__ == "__main__":
    unittest.main()

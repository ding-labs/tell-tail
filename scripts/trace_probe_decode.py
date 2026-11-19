"""Bounded OTLP/protobuf shape probe. Never returns payloads or raw identifiers.

This is a feasibility tool, not the production trace adapter. Wire field numbers
follow opentelemetry-proto logs/v1 and common/v1. Unknown fields are skipped;
only a fixed vocabulary and per-run keyed identifier digests leave the decoder.
"""

import hashlib
import re

MAX_BODY = 1024 * 1024
MAX_RECORDS = 1024
MAX_FIELDS = 4096
EVENTS = frozenset(("codex.tool_result", "codex.tool_decision",
                    "codex.conversation_starts", "codex.api_request"))
ATTRIBUTES = frozenset(("event.name", "tool_name", "call_id", "conversation.id",
                        "duration_ms", "success", "plugin_id", "connector_id",
                        "parent_call_id", "turn_id"))
# Exact probe operations only. A namespace prefix is not proof of attribution.
TOOLS = {"mcp__codex_apps__railway_search_docs": "railway.search_docs",
         "functions.exec": "functions.exec",
         "exec": "exec",
         "exec_command": "exec_command",
         "write_stdin": "write_stdin",
         "apply_patch": "apply_patch",
         "js": "js"}


class InvalidBatch(ValueError):
    """Fixed error; never include original bytes in error messages."""


def varint(data, pos):
    value = 0
    for shift in range(0, 70, 7):
        if pos >= len(data):
            raise InvalidBatch("invalid protobuf")
        byte = data[pos]
        pos += 1
        if shift == 63 and byte > 1:
            raise InvalidBatch("invalid protobuf")
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
    raise InvalidBatch("invalid protobuf")


def fields(data):
    pos = 0
    count = 0
    while pos < len(data):
        count += 1
        if count > MAX_FIELDS:
            raise InvalidBatch("field limit")
        tag, pos = varint(data, pos)
        number, wire = tag >> 3, tag & 7
        if not 0 < number < 2**29:
            raise InvalidBatch("invalid protobuf")
        if wire == 0:
            value, pos = varint(data, pos)
        else:
            if wire == 2:
                size, pos = varint(data, pos)
            elif wire in (1, 5):
                size = 8 if wire == 1 else 4
            else:
                raise InvalidBatch("unsupported protobuf wire type")
            end = pos + size
            if end > len(data):
                raise InvalidBatch("invalid protobuf")
            value, pos = data[pos:end], end
        yield number, wire, value


def scalar(data):
    result = None
    count = 0
    for number, wire, value in fields(data):
        count += 1
        if count > 1:  # AnyValue is a oneof; refuse ambiguous evidence.
            return None
        if number == 1 and wire == 2 and len(value) <= 256:
            try:
                result = bytes(value).decode("utf-8")
            except UnicodeDecodeError:
                return None
        elif number in (2, 3) and wire == 0:
            result = bool(value) if number == 2 else value
    return result


def attribute(data):
    key = value = None
    seen = set()
    for number, wire, raw in fields(data):
        if number in seen:
            raise InvalidBatch("duplicate attribute field")
        seen.add(number)
        if number == 1 and wire == 2 and len(raw) <= 64:
            try:
                key = bytes(raw).decode("utf-8")
            except UnicodeDecodeError:
                pass
        elif number == 2 and wire == 2:
            value = raw
    if key not in ATTRIBUTES:
        return None
    return key, scalar(value) if value is not None else None


def record(data, key, inspect_tool_names=False):
    attrs = {}
    names = []
    has_time = False
    for number, wire, raw in fields(data):
        if number in (1, 11) and wire == 1:
            has_time |= any(raw)
        elif number == 12 and wire == 2 and len(raw) <= 64:
            names.append(bytes(raw).decode("utf-8", errors="replace"))
        elif number == 5 and wire == 2:
            # Some SDKs place the event name in body; only exact known names survive.
            names.append(scalar(raw))
        elif number == 6 and wire == 2:
            item = attribute(raw)
            if item:
                name, value = item
                if name in attrs:
                    raise InvalidBatch("duplicate attribute")
                attrs[name] = value
    names.append(attrs.get("event.name"))
    known_names = {name for name in names if isinstance(name, str) and name in EVENTS}
    if len(known_names) > 1:
        raise InvalidBatch("conflicting event names")
    result = {"event": next(iter(known_names), "other"),
              "fields_present": sorted(attrs), "has_timestamp": bool(has_time),
              "tool": TOOLS.get(attrs.get("tool_name"), "unidentified")}
    tool_name = attrs.get("tool_name")
    if (inspect_tool_names and isinstance(tool_name, str)
            and re.fullmatch(r"[A-Za-z0-9_./:-]{1,128}", tool_name)):
        result["observed_tool_name"] = tool_name
    for name in ("tool_name", "call_id", "conversation.id", "parent_call_id", "turn_id"):
        value = attrs.get(name)
        if isinstance(value, str) and value:
            result[name + "_local"] = hashlib.blake2b(
                name.encode() + b"\0" + value.encode(), key=key, digest_size=16).hexdigest()
    duration = attrs.get("duration_ms")
    # tracing exporters can encode numeric fields as decimal strings.
    if isinstance(duration, str) and duration.isascii() and duration.isdecimal() and len(duration) <= 8:
        duration = int(duration)
    if isinstance(duration, int) and not isinstance(duration, bool) and 0 <= duration <= 86400000:
        result["duration_ms"] = duration
    success = attrs.get("success")
    if isinstance(success, bool):
        result["execution_success"] = success
    elif success in ("true", "false"):
        result["execution_success"] = success == "true"
    return result


def decode(body, key, inspect_tool_names=False):
    """Return only sanitized record summaries, rejecting over-budget batches."""
    if len(body) > MAX_BODY:
        raise InvalidBatch("body limit")
    records = []
    for number, wire, resource in fields(memoryview(body)):
        if (number, wire) != (1, 2):
            continue
        for number, wire, scope in fields(resource):
            if (number, wire) != (2, 2):
                continue
            for number, wire, raw in fields(scope):
                if (number, wire) != (2, 2):
                    continue
                if len(records) >= MAX_RECORDS:
                    raise InvalidBatch("record limit")
                records.append(record(raw, key, inspect_tool_names))
    return records

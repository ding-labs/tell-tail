#!/usr/bin/env python3
"""Finite loopback OTLP/protobuf probe for the plugin-tracing capture audit.

Creates a private configuration fragment, never edits client configuration, and
writes only sanitized metadata. Not a production collector or durable receiver.
"""

import argparse
import hmac
import json
import os
from pathlib import Path
import secrets
import socket
from http.server import BaseHTTPRequestHandler, HTTPServer
import threading
import time

from trace_probe_decode import InvalidBatch, MAX_BODY, decode


class Probe(HTTPServer):
    request_queue_size = 4

    def __init__(self, port=0, max_events=4096):
        super().__init__(("127.0.0.1", port), Handler)
        self.token = secrets.token_hex(32)
        self.key = secrets.token_bytes(32)
        self.records = []
        self.max_events = max_events
        self.requests = self.accepted = self.rejected = 0
        self.timeout = 0.2

    @property
    def authority(self):
        return f"127.0.0.1:{self.server_port}"

    def handle_error(self, request, client_address):
        # BaseServer would print a traceback, potentially with input-derived text.
        self.rejected += 1

    def report(self):
        return {"probe_schema_version": 1, "durable": False,
                "requests": self.requests, "accepted_batches": self.accepted,
                "rejected_requests": self.rejected,
                "retained_records": len(self.records), "records": self.records}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def handle(self):
        # Bound total request lifetime, including slowly trickled headers/body.
        def expire():
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self.connection.settimeout(2)
        timer = threading.Timer(2, expire)
        timer.start()
        try:
            super().handle()
        finally:
            timer.cancel()

    def send_error(self, code, message=None, explain=None):
        self.respond(code)

    def respond(self, status):
        self.close_connection = True
        self.server.requests += 1
        if status != 200:
            self.server.rejected += 1
        self.send_response(status)
        self.send_header("Content-Type", "application/x-protobuf")
        self.send_header("Content-Length", "0")
        self.send_header("Connection", "close")
        self.end_headers()
        # Empty protobuf ExportLogsServiceResponse indicates successful acceptance.

    def do_POST(self):
        headers = self.headers
        if any(len(headers.get_all(k, [])) != 1 for k in
               ("Host", "Content-Length", "Content-Type", "x-tell-tail-probe-key")):
            return self.respond(400)
        if headers.get("Host") != self.server.authority or "Origin" in headers:
            return self.respond(403)
        if not hmac.compare_digest(headers.get("x-tell-tail-probe-key", "").encode(),
                                   self.server.token.encode()):
            return self.respond(403)
        if self.path != "/v1/logs":
            return self.respond(404)
        if "Transfer-Encoding" in headers or "Content-Encoding" in headers:
            return self.respond(415)
        if headers.get("Content-Type") != "application/x-protobuf":
            return self.respond(415)
        try:
            length = int(headers["Content-Length"])
        except (ValueError, TypeError):
            return self.respond(400)
        if not 0 <= length <= MAX_BODY:
            return self.respond(413)
        body = self.rfile.read(length)
        if len(body) != length:
            return self.respond(400)
        try:
            records = decode(body, self.server.key)
        except InvalidBatch:
            return self.respond(400)
        if len(self.server.records) + len(records) > self.server.max_events:
            return self.respond(503)
        self.server.records.extend(records)
        self.server.accepted += 1
        return self.respond(200)


def private_fragment(path, probe):
    """Create, never overwrite. The fragment contains a local receiver credential."""
    config = ('[otel]\nlog_user_prompt = false\n'
              'exporter = { otlp-http = { endpoint = "http://' + probe.authority +
              '/v1/logs", protocol = "binary", headers = { "x-tell-tail-probe-key" = "' +
              probe.token + '" } } }\n')
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(config)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--seconds", type=int, default=60)
    parser.add_argument("--max-requests", type=int, default=256)
    parser.add_argument("--config-out", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 3600 or not 1 <= args.max_requests <= 4096:
        parser.error("duration or request budget out of range")
    if not 0 <= args.port <= 65535:
        parser.error("port out of range")
    # Reserve output before creating a receiver or fragment; neither file is overwritten.
    with args.output.open("x") as output, Probe(args.port) as probe:
        private_fragment(args.config_out, probe)
        print(json.dumps({"endpoint": f"http://{probe.authority}/v1/logs",
                          "configuration_fragment": str(args.config_out),
                          "seconds": args.seconds, "client_config_changed": False}), flush=True)
        deadline = time.monotonic() + args.seconds
        try:
            while time.monotonic() < deadline and probe.requests < args.max_requests:
                probe.handle_request()
        except KeyboardInterrupt:
            pass
        report = probe.report()
        json.dump(report, output, indent=2)
        output.write("\n")
        print(json.dumps({k: v for k, v in report.items() if k != "records"}), flush=True)


if __name__ == "__main__":
    main()

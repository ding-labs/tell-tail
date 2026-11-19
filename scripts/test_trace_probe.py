import http.client
import json
from pathlib import Path
import stat
import tempfile
import threading
import unittest

from trace_probe import Probe, private_fragment
from test_trace_probe_decode import attr, batch


class ProbeTests(unittest.TestCase):
    def setUp(self):
        self.probe = Probe(max_events=2)
        self.thread = threading.Thread(target=self.probe.serve_forever)
        self.thread.start()

    def tearDown(self):
        self.probe.shutdown()
        self.thread.join()
        self.probe.server_close()

    def post(self, body, headers=None, method="POST", path="/v1/logs"):
        h = {"Content-Type": "application/x-protobuf",
             "x-tell-tail-probe-key": self.probe.token}
        h.update(headers or {})
        conn = http.client.HTTPConnection(self.probe.authority, timeout=3)
        try:
            conn.request(method, path, body, h)
            response = conn.getresponse()
            result = response.status, response.read()
            return result
        finally:
            conn.close()

    def test_accepts_only_metadata_and_bounds_retention(self):
        data = batch(attr("event.name", "codex.tool_result") + attr("output", "PRIVATE"))
        self.assertEqual(self.post(data), (200, b""))
        self.assertEqual(self.post(data), (200, b""))
        self.assertEqual(self.post(data)[0], 503)
        report = self.probe.report()
        self.assertEqual(report["retained_records"], 2)
        self.assertEqual(report["rejected_requests"], 1)
        self.assertNotIn("PRIVATE", json.dumps(report))

    def test_access_and_protocol_rejections(self):
        for headers, status in [({"Origin": "http://localhost"}, 403),
                                ({"Host": "attacker.invalid"}, 403),
                                ({"x-tell-tail-probe-key": "wrong"}, 403),
                                ({"Content-Type": "application/json"}, 415),
                                ({"Content-Encoding": "gzip"}, 415),
                                ({"Content-Length": str(1024 * 1024 + 1)}, 413)]:
            self.assertEqual(self.post(b"", headers)[0], status)
        self.assertEqual(self.post(b"\x00")[0], 400)
        self.assertEqual(self.post(b"", path="/PRIVATE")[0], 404)
        status, body = self.post(b"", method="PRIVATE_METHOD")
        self.assertEqual(status, 501)
        self.assertNotIn(b"PRIVATE", body)
        self.assertEqual(self.probe.records, [])

    def test_private_fragment_never_overwrites(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / "probe.toml"
            private_fragment(p, self.probe)
            self.assertEqual(stat.S_IMODE(p.stat().st_mode), 0o600)
            import tomllib
            self.assertEqual(tomllib.loads(p.read_text())["otel"]["exporter"]["otlp-http"]["protocol"], "binary")
            with self.assertRaises(FileExistsError):
                private_fragment(p, self.probe)


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
import json
import pathlib
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from unittest.mock import patch

from soak_webhooks import EventLedger, Receiver, slack_sender, slack_text


def payload(recovered=False):
    return {'schema_version': 1, 'event': 'error_threshold_recovered' if recovered else 'error_threshold_met',
            'message': 'Recovered' if recovered else 'Check service', 'session_id': 'fixture',
            'source': 'stdin', 'observed_at_unix_seconds': 1001 if recovered else 1000,
            'window_seconds': 60, 'observed_seconds': 60.0, 'records': 500,
            'error_records': 100 if recovered else 250, 'error_percent': 20.0 if recovered else 50.0,
            'threshold_percent': 25.0}


class WebhookSoakTests(unittest.TestCase):
    def test_requires_ordered_qualified_transitions_in_one_session(self):
        ledger = EventLedger(60)
        with self.assertRaisesRegex(AssertionError, 'out-of-order'):
            ledger.accept(payload(True))
        ledger.accept(payload())
        for changes in [{'event': 'error_threshold_met'}, {'records': 99},
                        {'error_percent': 21.0}, {'session_id': 'restart'},
                        {'observed_at_unix_seconds': 999}, {'raw_log': 'private'}]:
            with self.assertRaises(AssertionError):
                ledger.accept({**payload(True), **changes})
        self.assertEqual(ledger.count, 1)
        ledger.accept(payload(True))
        self.assertEqual(ledger.count, 2)

    def test_formatter_provides_slack_text_and_distinguishes_recovery(self):
        self.assertIn('BREACH', slack_text(payload(), 'preflight'))
        text = slack_text(payload(True), '24-hour')
        self.assertIn('RECOVERY', text)
        self.assertIn('20.00%', text)
        self.assertIn('100 / 500', text)

    def test_receiver_captures_and_acknowledges_only_successful_slack_forwarding(self):
        for fail in [False, True]:
            with tempfile.TemporaryDirectory() as directory:
                events = pathlib.Path(directory) / 'events.ndjson'
                errors, sent = [], []
                def send(text):
                    sent.append(text)
                    if fail:
                        raise RuntimeError('Slack rejected notification')
                with Receiver(EventLedger(60), send, 'fixture', events, errors) as server:
                    worker = threading.Thread(target=server.handle_request)
                    worker.start()
                    request = urllib.request.Request(f'http://127.0.0.1:{server.server_port}/events',
                        data=json.dumps(payload()).encode(), headers={'Content-Type': 'application/json'})
                    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                    if fail:
                        with self.assertRaises(urllib.error.HTTPError) as result:
                            opener.open(request, timeout=3)
                        self.assertEqual(result.exception.code, 502)
                    else:
                        with opener.open(request, timeout=3) as response:
                            self.assertEqual(response.status, 204)
                    worker.join(timeout=3)
                    self.assertFalse(worker.is_alive())
                record = json.loads(events.read_text())
                self.assertEqual(record['slack_delivered'], not fail)
                self.assertEqual(bool(errors), fail)
                self.assertEqual(len(sent), 1)

    def test_slack_connection_errors_cannot_leak_the_webhook_secret(self):
        secret = 'https://hooks.slack.com/services/fixture/fixture/private-token'
        with tempfile.TemporaryDirectory() as directory:
            secret_file = pathlib.Path(directory) / 'secret'
            secret_file.write_text(secret)
            with patch('urllib.request.build_opener') as factory:
                factory.return_value.open.side_effect = urllib.error.URLError(secret)
                send = slack_sender(secret_file)
                with self.assertRaisesRegex(RuntimeError, '^Slack webhook connection failed$'):
                    send('fixture')


if __name__ == '__main__':
    unittest.main()

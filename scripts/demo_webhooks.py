#!/usr/bin/env python3
"""Show real breach and recovery webhooks using a temporary loopback receiver."""
import argparse
import http.server
import json
import os
import pathlib
import subprocess
import sys


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', default=str(root / 'target/debug/tell-tail'),
                        help='Tell-Tail executable (default: target/debug/tell-tail)')
    args = parser.parse_args()
    received = []

    class Receiver(http.server.BaseHTTPRequestHandler):
        def do_POST(self):
            self.connection.settimeout(2)
            length = int(self.headers['Content-Length'])
            if not 0 < length <= 16384:
                self.send_error(413)
                return
            received.append(json.loads(self.rfile.read(length)))
            self.send_response(204)
            self.end_headers()

        def log_message(self, *args):
            pass

    # Keep demo traffic local even when the shell has a proxy configured.
    environment = {key: value for key, value in os.environ.items()
                   if key.lower() not in ('http_proxy', 'https_proxy', 'all_proxy')}
    with http.server.HTTPServer(('127.0.0.1', 0), Receiver) as server:
        endpoint = f'http://127.0.0.1:{server.server_port}/tell-tail'
        command = [args.binary, '--output', 'json', '--interval-ms', '60000',
                   '--window', '60', '--alert-error-percent', '25',
                   '--alert-min-records', '100', '--alert-webhook', endpoint]
        print(f'Local receiver: {endpoint}', flush=True)
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env=environment)
        try:
            def feed(level, count):
                process.stdin.write((f'{level} demo record\n' * count).encode())
                process.stdin.flush()

            def receive(expected_event, expected_percent, expected_records):
                before = len(received)
                server.timeout = 3
                server.handle_request()
                if len(received) != before + 1:
                    raise RuntimeError(f'Timed out waiting for {expected_event}')
                payload = received[-1]
                expected = {'schema_version': 1, 'event': expected_event,
                            'error_percent': expected_percent, 'threshold_percent': 25.0,
                            'records': expected_records, 'error_records': 100}
                for field, value in expected.items():
                    if payload.get(field) != value:
                        raise RuntimeError(f'Unexpected {field}: {payload.get(field)!r}')
                # Print a short excerpt from the actual JSON received over HTTP.
                print('Webhook: ' + json.dumps({field: payload[field] for field in
                      ('event', 'error_percent', 'threshold_percent', 'message')}), flush=True)

            print('\n1. Healthy: 100 INFO records; 0% errors.', flush=True)
            feed('INFO', 100)
            server.timeout = 0.3
            server.handle_request()
            if received:
                raise RuntimeError('Healthy startup unexpectedly sent a webhook')
            print('   No webhook.', flush=True)

            print('\n2. Add 100 ERROR records: 100 / 200 = 50% errors.', flush=True)
            feed('ERROR', 100)
            receive('error_threshold_met', 50.0, 200)

            print('\n3. Add 300 INFO records: 100 / 500 = 20% errors.', flush=True)
            feed('INFO', 300)
            receive('error_threshold_recovered', 20.0, 500)
            if received[0]['session_id'] != received[1]['session_id']:
                raise RuntimeError('Breach and recovery belong to different sessions')

            # Check another live publication for duplicate recovery messages.
            server.timeout = 0.3
            server.handle_request()
            if len(received) != 2:
                raise RuntimeError('Recovery unexpectedly repeated')
            process.stdin.close()
            process.stdin = None
            stdout, stderr = process.communicate(timeout=3)
            if process.returncode != 0 or stderr:
                raise RuntimeError(f'Monitor failed: {stderr.decode(errors="replace")}')
            if json.loads(stdout)['totals']['records'] != 500:
                raise RuntimeError('Monitor did not count all 500 demo records')
            print('\nReceived one breach and one recovery; both acknowledged with HTTP 204.',
                  flush=True)
            print('Recovery arrived during the default 300-second breach cooldown.', flush=True)
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.communicate(timeout=1)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.communicate()


if __name__ == '__main__':
    try:
        main()
    except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
        print(f'Demo failed: {error}', file=sys.stderr)
        print('Build with cargo build --locked, or pass --binary /path/to/tell-tail.',
              file=sys.stderr)
        sys.exit(1)

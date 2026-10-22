#!/usr/bin/env python3
"""Run a bounded alert/recovery soak, capturing real webhooks and forwarding to Slack."""
import argparse
import datetime
import hashlib
import http.server
import json
import math
import pathlib
import threading
import urllib.error
import urllib.parse
import urllib.request

import soak


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def save(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


class EventLedger:
    def __init__(self, window):
        self.window = window
        self.count = 0
        self.session = None
        self.observed_at = 0

    def accept(self, payload):
        expected = 'error_threshold_met' if self.count % 2 == 0 else 'error_threshold_recovered'
        assert isinstance(payload, dict) and payload.get('schema_version') == 1, 'invalid webhook schema'
        assert payload.get('event') == expected, 'missing, repeated, or out-of-order transition'
        assert payload.get('source') == 'stdin', 'unexpected source'
        assert payload.get('window_seconds') == self.window, 'unexpected window'
        assert payload.get('threshold_percent') == 25, 'unexpected threshold'
        assert isinstance(payload.get('session_id'), str) and payload['session_id'], 'missing session'
        assert self.session is None or payload['session_id'] == self.session, 'monitor restarted'
        records, errors = payload.get('records'), payload.get('error_records')
        assert type(records) is int and records >= 100, 'insufficient sample'
        assert type(errors) is int and 0 <= errors <= records, 'invalid error count'
        percent = payload.get('error_percent')
        assert type(percent) in (int, float) and math.isfinite(percent), 'invalid percentage'
        assert math.isclose(percent, errors / records * 100, abs_tol=1e-9), 'incorrect percentage'
        assert (percent >= 25) == (expected == 'error_threshold_met'), 'incorrect transition'
        observed = payload.get('observed_at_unix_seconds')
        assert type(observed) in (int, float) and math.isfinite(observed), 'invalid observation time'
        assert observed >= self.observed_at, 'observation time moved backwards'
        assert set(payload) == {'schema_version', 'event', 'message', 'session_id', 'source',
                                'observed_at_unix_seconds', 'window_seconds', 'observed_seconds',
                                'records', 'error_records', 'error_percent', 'threshold_percent'}, 'unexpected payload fields'
        self.session = payload['session_id']
        self.observed_at = observed
        self.count += 1


def slack_text(payload, label):
    state = 'BREACH' if payload['event'] == 'error_threshold_met' else 'RECOVERY'
    return (f'Telltale soak · {label} · {state}\n'
            f'Error/fatal logs: {payload["error_percent"]:.2f}% '
            f'({payload["error_records"]:,} / {payload["records"]:,} records). '
            f'Threshold: {payload["threshold_percent"]:g}%; window: {payload["window_seconds"]}s.\n'
            f'{payload["message"]}')


def slack_sender(secret_file):
    url = secret_file.read_text().strip()
    parsed = urllib.parse.urlsplit(url)
    assert (parsed.scheme == 'https' and parsed.hostname == 'hooks.slack.com'
            and parsed.path.startswith('/services/') and not parsed.username
            and not parsed.password and not parsed.query and not parsed.fragment), 'invalid Slack webhook secret'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def send(text):
        request = urllib.request.Request(url, data=json.dumps({'text': text}).encode(),
                                         headers={'Content-Type': 'application/json'}, method='POST')
        try:
            with opener.open(request, timeout=2.5) as response:
                assert response.status == 200 and response.read(128).strip() == b'ok', 'Slack rejected notification'
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'Slack webhook returned HTTP {error.code}') from None
        except (OSError, urllib.error.URLError):
            # URL-bearing exceptions must never escape into reports or logs.
            raise RuntimeError('Slack webhook connection failed') from None
    return send


class Receiver(http.server.HTTPServer):
    def __init__(self, ledger, send, label, event_file, errors):
        self.ledger, self.send, self.label = ledger, send, label
        self.event_file, self.errors = event_file, errors
        super().__init__(('127.0.0.1', 0), ReceiverHandler)


class ReceiverHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.connection.settimeout(2)
        entry = {'received_at': now(), 'slack_delivered': False}
        try:
            assert self.path == '/events', 'unexpected receiver path'
            length = int(self.headers.get('Content-Length', '0'))
            assert 0 < length <= 16384, 'invalid webhook size'
            payload = json.loads(self.rfile.read(length))
            self.server.ledger.accept(payload)
            entry['payload'] = payload
            if self.server.send:
                self.server.send(slack_text(payload, self.server.label))
                entry['slack_delivered'] = True
            self.send_response(204)
        except Exception as error:
            entry['error'] = str(error)
            self.server.errors.append(str(error))
            self.send_response(502)
        with self.server.event_file.open('a') as stream:
            stream.write(json.dumps(entry) + '\n')
        self.end_headers()

    def log_message(self, *args):
        pass


def container_limits():
    root = pathlib.Path('/sys/fs/cgroup')
    names = ['memory.max', 'memory.swap.max', 'memory.peak', 'memory.events',
             'cpu.max', 'cpu.stat', 'cpuset.cpus.effective']
    return {name: (root / name).read_text().strip() for name in names if (root / name).is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--source-directory', type=pathlib.Path, required=True)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--results', type=pathlib.Path, required=True)
    parser.add_argument('--slack-webhook-file', type=pathlib.Path)
    parser.add_argument('--label', default='24-hour 1 GB / 1 CPU')
    parser.add_argument('--seconds', type=int, default=86400)
    parser.add_argument('--cycle-seconds', type=int, default=3600)
    parser.add_argument('--breach-start', type=int, default=120)
    parser.add_argument('--breach-seconds', type=int, default=120)
    parser.add_argument('--window', type=int, default=60)
    parser.add_argument('--rate', type=int, default=1000)
    parser.add_argument('--require-container-limits', action='store_true')
    options = parser.parse_args()
    if not (0 < options.seconds <= 86400 and options.cycle_seconds > 0
            and options.seconds % options.cycle_seconds == 0
            and options.cycle_seconds > options.breach_start + options.breach_seconds + options.window
            and options.breach_start >= options.window and options.breach_seconds >= options.window
            and (options.seconds == options.cycle_seconds or options.cycle_seconds >= 300)
            and 1 <= options.window <= 3600 and options.rate >= 100):
        parser.error('use complete cycles with healthy/breach/recovery windows and a 300s breach cooldown')
    options.binary = options.binary.resolve()
    options.source_directory = options.source_directory.resolve()
    options.results.mkdir(parents=True, exist_ok=False)
    report_path, journal_path = options.results / 'report.json', options.results / 'campaign.json'
    state = {'status': 'running', 'started_at': now(), 'duration_seconds': options.seconds,
             'source_commit': options.source_commit,
             'binary_sha256': hashlib.sha256(options.binary.read_bytes()).hexdigest(),
             'expected_webhooks': 2 * (options.seconds // options.cycle_seconds),
             'slack_enabled': bool(options.slack_webhook_file), 'container_limits': container_limits()}
    if options.require_container_limits:
        limits = state['container_limits']
        assert limits.get('memory.max') == '1073741824', 'expected a 1 GiB memory limit'
        assert limits.get('memory.swap.max') == '0', 'swap must be disabled'
        quota, period = map(int, limits['cpu.max'].split())
        assert quota == period and limits.get('cpuset.cpus.effective') == '0', 'expected one CPU'
    save(journal_path, state)
    errors, ledger = [], EventLedger(options.window)
    send = slack_sender(options.slack_webhook_file) if options.slack_webhook_file else None
    with Receiver(ledger, send, options.label, options.results / 'webhooks.ndjson', errors) as server:
        listener = threading.Thread(target=server.serve_forever, kwargs={'poll_interval': .05}, daemon=True)
        listener.start()
        arguments = argparse.Namespace(binary=options.binary, source_directory=options.source_directory,
            seconds=options.seconds, records=0, batch_copies=1, input='stdin', output='json',
            profile='alerting', seed=1, rate=options.rate, max_record_bytes=1048576, window=options.window,
            rss_limit_mib=64, disk_limit_mib=64, file_gib_per_day=10, rotate_bytes=8388608,
            exercise_terminal=False, alert_webhook=f'http://127.0.0.1:{server.server_port}/events',
            alert_cycle_seconds=options.cycle_seconds, alert_start_seconds=options.breach_start,
            alert_duration_seconds=options.breach_seconds, alert_cooldown=300, external_errors=errors)
        try:
            result = soak.run(arguments)
            assert not errors, errors
            assert ledger.count == state['expected_webhooks'], 'missing webhook transitions'
            assert result['production_seconds'] >= options.seconds, 'incomplete duration'
            assert result['producer_cpu_fraction'] <= .75, 'producer saturated its CPU budget'
            assert result['actual']['session_id'] == ledger.session, 'webhook/snapshot session mismatch'
            assert result['actual']['state'] == 'complete', 'monitor did not finish normally'
            result.update(status='passed', source_commit=options.source_commit,
                          webhook_count=ledger.count, slack_forwarded=ledger.count if send else 0,
                          container_limits=container_limits())
            if send:
                send(f'Telltale soak · {options.label} · PASSED\n'
                     f'{result["records"]:,} records; {ledger.count} breach/recovery notifications; '
                     f'peak monitor/helper RAM {result["peak_sampled_rss_mib"]:.2f} MiB. '
                     f'Full report retained locally.')
            save(report_path, result)
            state.update(status='passed', finished_at=now(), webhook_count=ledger.count)
        except Exception as error:
            failure = {'status': 'failed', 'error': str(error), 'finished_at': now(),
                       'webhook_count': ledger.count, **getattr(error, 'soak_details', {})}
            save(report_path, failure)
            state.update(status='failed', finished_at=now(), error=str(error), webhook_count=ledger.count)
            if send:
                try:
                    send(f'Telltale soak · {options.label} · FAILED\n{str(error)[:1500]}\n'
                         'This run does not count as uninterrupted duration evidence.')
                except Exception:
                    state['failure_notification_delivered'] = False
            raise
        finally:
            state['updated_at'] = now()
            save(journal_path, state)
            server.shutdown()
            listener.join(timeout=3)
    print(json.dumps({'status': state['status'], 'results': str(options.results),
                      'webhook_count': ledger.count}), flush=True)


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Watch one owned soak container across Docker disconnects; never restart it."""
import argparse
import datetime
import json
import pathlib
import subprocess
import time


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def save(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def reconcile_exit(results, container):
    path = results / 'campaign.json'
    journal = json.loads(path.read_text()) if path.exists() else {'phases': []}
    if journal.get('status') == 'passed' and container['ExitCode'] == 0:
        return 0
    if journal.get('status') in ('failed', 'interrupted'):
        return 1
    original = results / 'campaign.before-interruption.json'
    if not original.exists():
        save(original, journal)
    journal.update(status='interrupted', updated_at=now(),
                   reason='Container exited without a completed campaign journal',
                   container_state=container)
    for phase in journal.get('phases', []):
        if phase.get('status') == 'running':
            phase.update(status='interrupted', container_exit_code=container['ExitCode'])
    save(path, journal)
    return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=pathlib.Path, required=True)
    parser.add_argument('--poll-seconds', type=float, default=15)
    parser.add_argument('--timeout-seconds', type=float, default=49 * 3600)
    args = parser.parse_args()
    if not 0 < args.poll_seconds <= 60 or args.timeout_seconds <= 0:
        parser.error('invalid polling interval or timeout')
    config = json.loads((args.campaign / 'config.json').read_text())
    results = args.campaign / 'results'
    deadline = time.monotonic() + args.timeout_seconds
    while time.monotonic() < deadline:
        try:
            state = json.loads(subprocess.check_output(
                ['docker', 'inspect', config['container'], '--format', '{{json .State}}'],
                text=True, stderr=subprocess.PIPE, timeout=10))
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            save(results / 'host-monitor.json', {'checked_at': now(),
                 'status': 'waiting_for_docker', 'error': str(error)})
            # The host watcher (and its caffeinate wrapper) survives a lost API.
            time.sleep(args.poll_seconds)
            continue
        if not state['Running']:
            code = reconcile_exit(results, state)
            save(results / 'host-monitor.json', {'checked_at': now(),
                 'status': 'finished' if code == 0 else 'interrupted', 'container_state': state})
            return code
        save(results / 'host-monitor.json', {'checked_at': now(),
             'status': 'paused' if state.get('Paused') else 'running', 'container_state': state})
        time.sleep(args.poll_seconds)
    save(results / 'host-monitor.json', {'checked_at': now(), 'status': 'watcher_timeout'})
    return 2


if __name__ == '__main__':
    raise SystemExit(main())

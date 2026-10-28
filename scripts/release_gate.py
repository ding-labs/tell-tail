#!/usr/bin/env python3
"""Require recorded validation for the exact stable version and source commit."""
import argparse
import datetime
import json
import re

GATES = frozenset(('correctness', 'fuzzing', 'linux_day', 'dashboard_hour',
                   'default_memory', 'maximum_memory', 'responsiveness',
                   'performance', 'compatibility', 'distribution'))
VERSION = r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)'


def valid_day_waiver(waiver, version, commit):
    """Only a maintainer-recorded, candidate-specific Linux day waiver is supported."""
    if not isinstance(waiver, dict):
        return False
    if waiver.get('version') != version or waiver.get('source_commit') != commit:
        return False
    if not all(isinstance(waiver.get(key), str) and waiver[key].strip()
               for key in ('reason', 'approved_by', 'approved_at')):
        return False
    try:
        approved = datetime.datetime.fromisoformat(waiver['approved_at'])
    except ValueError:
        return False
    return approved.utcoffset() is not None


def validate(record, version, commit):
    if not re.fullmatch(VERSION, version):
        raise ValueError('a stable x.y.z version is required')
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('a full source commit SHA is required')
    if record.get('schema_version') != 1:
        raise ValueError('unsupported validation schema')
    if record.get('version') != version or record.get('source_commit') != commit:
        raise ValueError('validation does not cover this version and source commit')
    gates = record.get('gates', {})
    if set(gates) != GATES:
        raise ValueError('validation must cover every required release gate')
    waivers = record.get('waivers', {})
    if not isinstance(waivers, dict) or set(waivers) - {'linux_day'}:
        raise ValueError('only an explicit Linux day waiver is supported')
    if 'linux_day' in waivers:
        if (gates['linux_day'].get('passed') is not False
                or not valid_day_waiver(waivers['linux_day'], version, commit)):
            raise ValueError('Linux day waiver must be complete, candidate-specific and recorded as unpassed')
    for name, gate in gates.items():
        evidence = gate.get('evidence')
        waived = (name == 'linux_day' and gate.get('passed') is False
                  and valid_day_waiver(waivers.get(name), version, commit))
        if ((gate.get('passed') is not True and not waived) or not isinstance(evidence, list)
                or not evidence or not all(isinstance(item, str) and item.strip() for item in evidence)):
            raise ValueError(f'release gate has not passed with evidence: {name}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--validation', required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    try:
        with open(args.validation) as source:
            record = json.load(source)
            validate(record, args.version, args.commit)
    except (ValueError, OSError, TypeError, AttributeError) as error:
        parser.exit(1, f'Release blocked: {error}\n')
    print(f'Validation covers {args.version} at {args.commit}')
    for name, waiver in record.get('waivers', {}).items():
        print(f'Explicit waiver for {name}: {waiver["approved_by"]} at {waiver["approved_at"]}: {waiver["reason"]}')


if __name__ == '__main__':
    main()

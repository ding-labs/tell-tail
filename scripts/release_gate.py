#!/usr/bin/env python3
"""Require recorded validation for the exact stable version and source commit."""
import argparse
import json
import re

GATES = frozenset(('correctness', 'fuzzing', 'linux_day', 'dashboard_hour',
                   'default_memory', 'maximum_memory', 'responsiveness',
                   'performance', 'compatibility', 'distribution'))
VERSION = r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)'


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
    for name, gate in gates.items():
        evidence = gate.get('evidence')
        if (gate.get('passed') is not True or not isinstance(evidence, list)
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
            validate(json.load(source), args.version, args.commit)
    except (ValueError, OSError, TypeError, AttributeError) as error:
        parser.exit(1, f'Release blocked: {error}\n')
    print(f'Validation covers {args.version} at {args.commit}')


if __name__ == '__main__':
    main()

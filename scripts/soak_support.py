"""Deterministic traffic and process-tree accounting for validation tools."""
from dataclasses import dataclass, field
import hashlib
import json
import pathlib
import random
import shutil
import subprocess

LEVELS = ('trace', 'debug', 'info', 'warn', 'error', 'fatal', 'unknown')


class ObservationClock:
    """A suspended host must not turn an interrupted run into duration evidence."""
    def __init__(self, monotonic, wall, budget=5.0):
        self.previous = (monotonic, wall)
        self.budget = budget
        self.maximum_gap = 0.0

    def observe(self, monotonic, wall):
        gap = max(monotonic - self.previous[0], wall - self.previous[1])
        self.maximum_gap = max(self.maximum_gap, gap)
        self.previous = (monotonic, wall)
        if gap > self.budget:
            raise AssertionError(f'observation gap exceeded {self.budget:g}s: {gap:.3f}s')


@dataclass
class Counts:
    levels: dict = field(default_factory=lambda: dict.fromkeys(LEVELS, 0))
    bytes_read: int = 0
    parse_failures: int = 0
    oversized_records: int = 0
    skipped_bytes: int = 0
    discarded_partial_records: int = 0

    @property
    def records(self):
        return sum(self.levels.values())

    def add(self, other):
        for name in LEVELS:
            self.levels[name] += other.levels[name]
        for name in ('bytes_read', 'parse_failures', 'oversized_records',
                     'skipped_bytes', 'discarded_partial_records'):
            setattr(self, name, getattr(self, name) + getattr(other, name))

    def verify(self, report):
        expected = {'levels': self.levels, 'records': self.records,
                    'parse_failures': self.parse_failures,
                    'oversized_records': self.oversized_records}
        assert report['totals'] == expected, (report['totals'], expected)
        for key in ('bytes_read', 'skipped_bytes', 'discarded_partial_records'):
            assert report[key] == getattr(self, key), (key, report[key], getattr(self, key))
        assert report['pending_bytes'] == 0, report['pending_bytes']


def make_batch(seed, profile, limit):
    if profile == 'alerting':
        return make_alert_batch(seed, False)
    rng = random.Random(seed)
    levels = ['info'] * 90 + ['warn'] * 9 + ['error']
    rng.shuffle(levels)
    records = []
    counts = Counts()
    for index, level in enumerate(levels):
        failure = False
        if profile == 'sustained':
            line = level.upper().encode() + b' ' + b'x' * (198 - len(level))
        else:
            kind = index % 10
            if kind < 7:
                level = LEVELS[kind]
                message = 'x' * rng.randrange(8, 512)
                line = (json.dumps({'level': level, 'message': message}, separators=(',', ':')).encode()
                        if index % 2 else f'[{level.upper()}] {message}'.encode())
            elif kind == 7:
                line, level, failure = b'{"level":"INFO","message":"\\uD800"}', 'unknown', True
            elif kind == 8:
                line, level, failure = b'{"level":"INFO","message":"\xff"}', 'unknown', True
            else:
                line, level = b'level=error message=unsupported', 'unknown'
            if index == 99:
                line, level, failure = b'x' * (limit + 1), 'unknown', False
        oversized = len(line) > limit
        counts.levels['unknown' if oversized else level] += 1
        counts.parse_failures += int(failure and not oversized)
        counts.oversized_records += int(oversized)
        records.append(line + b'\n')
    payload = b''.join(records)
    counts.bytes_read = len(payload)
    return payload, counts, hashlib.sha256(payload).hexdigest()


def make_alert_batch(seed, breached):
    """A repeatable 100-record mix of JSON/text, with 1% or 50% error/fatal."""
    levels = (['info'] * 50 + ['error'] * 40 + ['fatal'] * 10 if breached else
              ['info'] * 89 + ['warn'] * 10 + ['error'])
    random.Random(seed).shuffle(levels)
    counts = Counts()
    lines = []
    for index, level in enumerate(levels):
        line = (json.dumps({'level': level, 'message': 'x' * 160}, separators=(',', ':'))
                if index % 2 else f'[{level.upper()}] ' + 'x' * 192)
        lines.append(line.encode() + b'\n')
        counts.levels[level] += 1
    payload = b''.join(lines)
    counts.bytes_read = len(payload)
    return payload, counts, hashlib.sha256(payload).hexdigest()


def alert_phase(elapsed, cycle, start, duration):
    return start <= elapsed % cycle < start + duration


def cpu_seconds(value):
    days, _, clock = value.rpartition('-')
    result = int(days or 0) * 86400
    fields = clock.split(':')
    for index, part in enumerate(reversed(fields)):
        result += float(part) * 60 ** index
    return result


def select_tree(rows, root_pid):
    selected = {root_pid}
    while True:
        descendants = {pid for pid, parent, *_ in rows if parent in selected}
        if descendants <= selected:
            break
        selected |= descendants
    return [row for row in rows if row[0] in selected]


def resources(root_pid, include_fds=False):
    result = subprocess.run(['ps', '-axo', 'pid=,ppid=,rss=,time='],
                            capture_output=True, text=True, check=True)
    rows = []
    for line in result.stdout.splitlines():
        pid, parent, rss, cpu = line.split()
        rows.append((int(pid), int(parent), int(rss), cpu_seconds(cpu)))
    tree = select_tree(rows, root_pid)
    if not tree:
        return None
    pids = [row[0] for row in tree]
    fds = None
    if include_fds:
        if pathlib.Path('/proc').is_dir():
            try:
                fds = sum(len(list(pathlib.Path(f'/proc/{pid}/fd').iterdir())) for pid in pids)
            except FileNotFoundError:
                pass
        elif shutil.which('lsof'):
            result = subprocess.run(['lsof', '-n', '-P', '-a', '-p', ','.join(map(str, pids)), '-F', 'f'],
                                    capture_output=True, text=True)
            if result.returncode == 0:
                fds = sum(line[1:].isdigit() for line in result.stdout.splitlines() if line.startswith('f'))
    return {'rss_mib': sum(row[2] for row in tree) / 1024,
            'sampled_cpu_seconds': sum(row[3] for row in tree),
            'processes': len(tree), 'open_descriptors': fds}

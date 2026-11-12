//! Bounded evidence, mirror matching and execution-scoped failure groups.
use super::{
    collection::{Record, SourceView},
    discovery::RootView,
    model::*,
    parse,
    time::now_ms,
};
use serde::Serialize;
use std::{
    collections::{VecDeque, hash_map::RandomState},
    hash::BuildHasher,
};

#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub id: u64,
    pub host: Host,
    pub server: String,
    pub scope: String,
    pub phase: &'static str,
    pub reason: Reason,
    pub first_ms: i64,
    pub last_ms: i64,
    pub observations: u64,
    pub last_event_id: u64,
    pub historical: bool,
    pub recovered_ms: Option<i64>,
    pub advice: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct ServerState {
    pub host: Host,
    pub server: String,
    pub scope: String,
    pub kind: Kind,
    pub timestamp_ms: i64,
    pub historical: bool,
    pub event_id: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    pub observations: u64,
    pub mirrors: u64,
    pub ignored: u64,
    pub unknown: u64,
    pub evicted_events: u64,
    pub evicted_issues: u64,
    pub evicted_states: u64,
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub sequence: u64,
    pub observed_ms: i64,
    pub running: bool,
    pub roots: Vec<RootView>,
    pub sources: Vec<SourceView>,
    pub discovery_limited: bool,
    pub rejected_files: usize,
    pub open_failures: u64,
    pub retired_sources: u64,
    pub counts: Counts,
    pub events: Vec<Event>,
    pub issues: Vec<Issue>,
    pub servers: Vec<ServerState>,
    pub oldest_event_id: Option<u64>,
    pub retained_events: usize,
    pub retained_issues: usize,
    pub retained_states: usize,
    pub view_omitted: usize,
}
pub struct Store {
    pub events: VecDeque<Event>,
    pub issues: VecDeque<Issue>,
    pub servers: VecDeque<ServerState>,
    pub counts: Counts,
    pub roots: Vec<RootView>,
    pub sources: Vec<SourceView>,
    pub discovery_limited: bool,
    pub rejected_files: usize,
    pub open_failures: u64,
    pub retired_sources: u64,
    pub running: bool,
    pub sequence: u64,
    next_id: u64,
    hash: RandomState,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            issues: VecDeque::new(),
            servers: VecDeque::new(),
            counts: Counts::default(),
            roots: vec![],
            sources: vec![],
            discovery_limited: false,
            rejected_files: 0,
            open_failures: 0,
            retired_sources: 0,
            running: true,
            sequence: 0,
            next_id: 1,
            hash: RandomState::new(),
        }
    }
}
impl Store {
    pub fn accept(&mut self, r: Record<'_>) {
        let Some(p) = parse::record(r.host, r.bytes) else {
            self.counts.ignored += 1;
            return;
        };
        let evidence = Evidence {
            source_id: r.source_id,
            generation: r.generation,
            offset: r.offset,
        };
        if p.host == Host::ClaudeDesktop
            && let Some(stamp) = p.timestamp_ms
        {
            let mirror = self.events.iter_mut().find(|e| {
                e.host == p.host
                    && e.server == p.server
                    && e.level == p.level
                    && e.aggregate != p.aggregate
                    && e.fingerprint == p.fingerprint
                    && e.evidence.len() == 1
                    && e.evidence[0].source_id != r.source_id
                    && e.timestamp_ms.is_some_and(|t| t.abs_diff(stamp) <= 50)
            });
            if let Some(e) = mirror {
                if p.aggregate {
                    e.evidence.push(evidence);
                } else {
                    e.evidence.insert(0, evidence);
                }
                self.counts.mirrors += 1;
                self.sequence += 1;
                return;
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        let prior_start = self
            .events
            .iter()
            .rev()
            .filter(|e| {
                e.host == p.host
                    && e.server == p.server
                    && e.kind == Kind::Starting
                    && e.timestamp_ms
                        .zip(p.timestamp_ms)
                        .is_some_and(|(a, b)| a <= b)
            })
            .max_by_key(|e| (e.timestamp_ms, e.id));
        let scope_known = p.scope.is_some()
            || (p.host == Host::ClaudeDesktop
                && (p.kind == Kind::Starting || prior_start.is_some()));
        let scope = if let Some(scope) = p.scope.as_deref() {
            format!("{:016x}", self.hash.hash_one((p.host, &p.server, scope)))
        } else if p.host == Host::ClaudeDesktop && p.kind == Kind::Starting {
            format!("{:016x}", self.hash.hash_one((p.host, &p.server, id)))
        } else if p.host == Host::ClaudeDesktop
            && let Some(start) = prior_start
        {
            start.scope.clone()
        } else {
            format!(
                "{:016x}",
                self.hash
                    .hash_one((p.host, &p.server, r.source_id, r.generation, "unknown"))
            )
        };
        let summary = match p.reason {
            Some(reason) => format!("{}: {}", p.kind.label(), reason.label()),
            None => p.kind.label().into(),
        };
        let event = Event {
            id,
            host: p.host,
            server: p.server,
            scope,
            scope_known,
            timestamp_ms: p.timestamp_ms,
            observed_ms: now_ms(),
            historical: r.historical,
            level: p.level,
            kind: p.kind,
            reason: p.reason,
            summary,
            method: p.method,
            evidence: vec![evidence],
            payload_available: false,
            fingerprint: p.fingerprint,
            aggregate: p.aggregate,
        };
        self.update_state(&event);
        self.events.push_back(event);
        self.counts.observations += 1;
        self.counts.unknown += u64::from(p.kind == Kind::Unknown);
        if self.events.len() > MAX_EVENTS {
            self.events.pop_front();
            self.counts.evicted_events += 1;
        }
        self.sequence += 1;
    }
    fn update_state(&mut self, e: &Event) {
        let at = e.timestamp_ms.unwrap_or(e.observed_ms);
        if let Some(reason) = e.reason {
            let issue = self.issues.iter_mut().find(|i| {
                i.host == e.host
                    && i.server == e.server
                    && i.scope == e.scope
                    && i.phase == e.kind.phase()
                    && i.reason == reason
            });
            if let Some(i) = issue {
                i.observations += 1;
                i.first_ms = i.first_ms.min(at);
                if at >= i.last_ms {
                    i.last_ms = at;
                    i.last_event_id = e.id;
                    i.historical = e.historical;
                }
                if i.recovered_ms.is_some_and(|r| at > r) {
                    i.recovered_ms = None;
                }
            } else {
                self.issues.push_back(Issue {
                    id: e.id,
                    host: e.host,
                    server: e.server.clone(),
                    scope: e.scope.clone(),
                    phase: e.kind.phase(),
                    reason,
                    first_ms: at,
                    last_ms: at,
                    observations: 1,
                    last_event_id: e.id,
                    historical: e.historical,
                    recovered_ms: if e.scope_known
                        && e.timestamp_ms.is_some()
                        && matches!(e.kind.phase(), "startup" | "connection")
                    {
                        self.events
                            .iter()
                            .filter(|r| {
                                r.host == e.host
                                    && r.server == e.server
                                    && r.scope == e.scope
                                    && r.scope_known
                                    && r.kind == Kind::Ready
                            })
                            .filter_map(|r| r.timestamp_ms)
                            .filter(|t| *t >= at)
                            .max()
                    } else {
                        None
                    },
                    advice: reason.advice(),
                });
                if self.issues.len() > MAX_ISSUES {
                    self.issues.pop_front();
                    self.counts.evicted_issues += 1;
                }
            }
        }
        if e.kind == Kind::Ready && e.scope_known && e.timestamp_ms.is_some() {
            for i in &mut self.issues {
                if i.host == e.host
                    && i.server == e.server
                    && i.scope == e.scope
                    && matches!(i.phase, "startup" | "connection")
                    && i.last_ms <= at
                {
                    i.recovered_ms = Some(at);
                }
            }
        }
        if matches!(
            e.kind,
            Kind::Protocol | Kind::Unknown | Kind::DiagnosticFailed | Kind::DiscoveryFailed
        ) {
            return;
        }
        let key = |s: &ServerState| s.host == e.host && s.server == e.server && s.scope == e.scope;
        let state = ServerState {
            host: e.host,
            server: e.server.clone(),
            scope: e.scope.clone(),
            kind: e.kind,
            timestamp_ms: at,
            historical: e.historical,
            event_id: e.id,
        };
        if let Some(index) = self.servers.iter().position(key) {
            if self.servers[index].timestamp_ms <= at {
                self.servers.remove(index);
                self.servers.push_back(state);
            }
        } else {
            self.servers.push_back(state);
        }
        if self.servers.len() > MAX_ISSUES {
            self.servers.pop_front();
            self.counts.evicted_states += 1;
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut issues: Vec<_> = self.issues.iter().cloned().collect();
        issues.sort_by_key(|i| std::cmp::Reverse(i.last_ms));
        issues.truncate(40);
        Snapshot {
            schema_version: SCHEMA_VERSION,
            sequence: self.sequence,
            observed_ms: now_ms(),
            running: self.running,
            roots: self.roots.clone(),
            sources: self.sources.clone(),
            discovery_limited: self.discovery_limited,
            rejected_files: self.rejected_files,
            open_failures: self.open_failures,
            retired_sources: self.retired_sources,
            counts: self.counts.clone(),
            events: self.events.iter().rev().take(100).cloned().collect(),
            issues,
            servers: self.servers.iter().rev().take(40).cloned().collect(),
            oldest_event_id: self.events.front().map(|e| e.id),
            retained_events: self.events.len(),
            retained_issues: self.issues.len(),
            retained_states: self.servers.len(),
            view_omitted: 0,
        }
    }
}
impl Snapshot {
    /// Independently bound transport size even at maximum source/state cardinality.
    pub fn json(mut self) -> Result<Vec<u8>, serde_json::Error> {
        loop {
            let bytes = serde_json::to_vec(&self)?;
            if bytes.len() <= 64 * 1024 {
                return Ok(bytes);
            }
            if self.events.pop().is_none()
                && self.issues.pop().is_none()
                && self.servers.pop().is_none()
            {
                // Source names and roots are bounded by construction; this is unreachable.
                return Err(serde_json::Error::io(std::io::Error::other(
                    "MCP overview exceeds byte budget",
                )));
            }
            self.view_omitted += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(s: &mut Store, source: u64, host: Host, line: &str) {
        s.accept(Record {
            source_id: source,
            generation: 0,
            offset: 0,
            historical: true,
            host,
            bytes: line.as_bytes(),
        });
    }
    #[test]
    fn mirrors_pair_once_and_keep_legitimate_repetitions() {
        let mut s = Store::default();
        for _ in 0..675 {
            put(
                &mut s,
                1,
                Host::ClaudeDesktop,
                "2026-11-12T00:00:00.000Z [info] [example] Client transport closed",
            );
        }
        for _ in 0..675 {
            put(
                &mut s,
                2,
                Host::ClaudeDesktop,
                "2026-11-12T00:00:00.031Z [example] [info] Client transport closed { metadata: undefined }",
            );
        }
        assert_eq!(s.events.len(), 675);
        assert_eq!(s.counts.mirrors, 675);
        assert_eq!(s.issues.len(), 0);
        assert!(
            s.events
                .iter()
                .all(|e| e.evidence.len() == 2 && e.evidence[0].source_id == 2)
        );
    }
    #[test]
    fn failures_group_and_recovery_cannot_cross_executions() {
        let mut s = Store::default();
        let fail = "2026-11-12T00:00:00Z warning [AppServerConnection] mcp_server_startup_status_updated server=example status=failed failureReason=reauthenticationRequired error=\"PRIVATE_TOKEN\" threadId=PRIVATE_A";
        put(&mut s, 1, Host::CodexDesktop, fail);
        put(&mut s, 1, Host::CodexDesktop, fail);
        put(
            &mut s,
            1,
            Host::CodexDesktop,
            "2026-11-12T00:00:01Z info [AppServerConnection] mcp_server_startup_status_updated server=example status=ready error=null threadId=PRIVATE_B",
        );
        assert_eq!(s.issues.len(), 1);
        assert_eq!(s.issues[0].observations, 2);
        assert!(s.issues[0].recovered_ms.is_none());
        put(
            &mut s,
            1,
            Host::CodexDesktop,
            "2026-11-12T00:00:02Z info [AppServerConnection] mcp_server_startup_status_updated server=example status=ready error=null threadId=PRIVATE_A",
        );
        assert!(s.issues[0].recovered_ms.is_some());
        let json = String::from_utf8(s.snapshot().json().unwrap()).unwrap();
        assert!(!json.contains("PRIVATE"));
    }
    #[test]
    fn normal_closes_and_two_error_lines_are_one_issue() {
        let mut s = Store::default();
        for i in 0..43 {
            let ts = format!("2026-11-12T00:{i:02}:00Z");
            put(
                &mut s,
                1,
                Host::ClaudeDesktop,
                &format!("{ts} [example] [info] Initializing server..."),
            );
            let body = if i < 34 {
                "Server transport closed (intentional shutdown)"
            } else if i < 42 {
                "Server transport closed (renderer released port)"
            } else {
                "Server transport closed unexpectedly"
            };
            put(
                &mut s,
                1,
                Host::ClaudeDesktop,
                &format!("{ts} [example] [info] {body}"),
            );
        }
        put(
            &mut s,
            1,
            Host::ClaudeDesktop,
            "2026-11-12T00:42:00Z [example] [error] Server disconnected.",
        );
        assert_eq!(s.issues.len(), 1);
        assert_eq!(s.issues[0].observations, 2);
    }
    #[test]
    fn retention_and_frames_are_bounded() {
        let mut s = Store::default();
        for n in 0..2100 {
            put(
                &mut s,
                1,
                Host::CodexDesktop,
                &format!(
                    "2026-11-12T00:00:00Z warning [AppServerConnection] mcp_server_startup_status_updated server=example status=failed threadId={n}"
                ),
            );
        }
        assert_eq!(s.events.len(), MAX_EVENTS);
        assert_eq!(s.issues.len(), MAX_ISSUES);
        assert_eq!(s.servers.len(), MAX_ISSUES);
        assert_eq!(s.counts.evicted_events, 100);
        assert!(s.snapshot().json().unwrap().len() <= 64 * 1024);
    }
}

//! Bounded current calls, revision snapshots, and source-scoped delivery dedup.
use super::{correlate::Tracked, identity::Identity, model::*};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_CALLS: usize = 2_000;
const MAX_REVISIONS: usize = 8_000;
const MAX_PENDING: usize = 512;
const MAX_SOURCES: usize = 64;
const MAX_DEDUP: usize = 4_096;
// Conservative per-entry charge includes strings and index/node overhead.
const ENTRY_CHARGE: usize = 2_048;
const MAX_CHARGED_BYTES: usize = 16 * 1024 * 1024;

#[derive(Default, Clone, Debug, Serialize)]
pub struct Counts {
    #[serde(serialize_with = "decimal")]
    pub accepted_observations: u64,
    #[serde(serialize_with = "decimal")]
    pub distinct_calls: u64,
    #[serde(serialize_with = "decimal")]
    pub duplicate_observations: u64,
    #[serde(serialize_with = "decimal")]
    pub rejected_observations: u64,
    #[serde(serialize_with = "decimal")]
    pub evicted_calls: u64,
    #[serde(serialize_with = "decimal")]
    pub evicted_revisions: u64,
    #[serde(serialize_with = "decimal")]
    pub incomplete_calls: u64,
}

#[derive(Default, Clone, Hash)]
pub struct Filter {
    pub plugin: Option<String>,
    pub tool: Option<String>,
    pub status: Option<Status>,
    pub execution: Option<String>,
    pub search: Option<String>,
    pub since_ms: Option<i64>,
}
impl Filter {
    fn matches(&self, c: &Call) -> bool {
        self.plugin
            .as_ref()
            .is_none_or(|v| c.plugin.as_ref() == Some(v))
            && self.tool.as_ref().is_none_or(|v| &c.tool == v)
            && self.status.is_none_or(|v| c.status == v)
            && self.execution.as_ref().is_none_or(|v| &c.execution == v)
            && self
                .since_ms
                .is_none_or(|v| c.started_ms.or(c.completed_ms).unwrap_or(c.received_ms) >= v)
            && self.search.as_ref().is_none_or(|v| {
                format!(
                    "{} {} {} {}",
                    c.tool,
                    c.plugin.as_deref().unwrap_or(""),
                    c.server.as_deref().unwrap_or(""),
                    c.execution
                )
                .to_lowercase()
                .contains(&v.to_lowercase())
            })
    }
}

#[derive(Serialize)]
pub struct Page {
    pub schema_version: u32,
    pub collector_epoch: String,
    #[serde(serialize_with = "decimal")]
    pub as_of: u64,
    pub matched: usize,
    pub calls: Vec<Call>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub collector_epoch: String,
    #[serde(serialize_with = "decimal")]
    pub watermark: u64,
    #[serde(serialize_with = "decimal")]
    pub oldest_watermark: u64,
    pub retained_calls: usize,
    pub retained_revisions: usize,
    pub charged_bytes: usize,
    pub counts: Counts,
}

pub struct Store {
    identity: Identity,
    epoch: String,
    calls: BTreeMap<u64, Tracked>,
    by_identity: BTreeMap<String, u64>,
    revisions: VecDeque<Call>,
    deliveries: BTreeMap<String, String>,
    delivery_order: VecDeque<String>,
    sources: BTreeSet<String>,
    sequence: u64,
    next_id: u64,
    floor: u64,
    counts: Counts,
    has_private_context: bool,
    max_calls: usize,
    max_revisions: usize,
}
impl Default for Store {
    fn default() -> Self {
        let identity = Identity::default();
        let epoch = identity.label("collector", 0);
        Self {
            identity,
            epoch,
            calls: BTreeMap::new(),
            by_identity: BTreeMap::new(),
            revisions: VecDeque::new(),
            deliveries: BTreeMap::new(),
            delivery_order: VecDeque::new(),
            sources: BTreeSet::new(),
            sequence: 0,
            next_id: 1,
            floor: 0,
            counts: Counts::default(),
            has_private_context: false,
            max_calls: MAX_CALLS,
            max_revisions: MAX_REVISIONS,
        }
    }
}
impl Store {
    /// Suggestions are derived from bounded retained evidence, never plugin-cache guesses.
    pub fn observed_plugins(&self) -> Vec<String> {
        self.calls
            .values()
            .filter_map(|t| t.call.plugin.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .take(128)
            .collect()
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            collector_epoch: self.epoch.clone(),
            watermark: self.sequence,
            oldest_watermark: self.floor,
            retained_calls: self.calls.len(),
            retained_revisions: self.revisions.len(),
            charged_bytes: self.charged_bytes(),
            counts: self.counts.clone(),
        }
    }
    /// Current call updates in revision order; callers must expose retention
    /// gaps using oldest_watermark rather than treating this as a durable log.
    pub fn updates_after(&self, sequence: u64) -> Vec<Call> {
        let mut values: Vec<_> = self
            .calls
            .values()
            .filter(|t| t.call.revision > sequence)
            .map(|t| t.call.clone())
            .collect();
        values.sort_by_key(|c| c.revision);
        values
    }
    fn charged_bytes(&self) -> usize {
        (self.calls.len() + self.revisions.len())
            * (ENTRY_CHARGE + if self.has_private_context { 2048 } else { 0 })
            + self.deliveries.len() * 256
            + self.sources.len() * 128
    }
    pub fn accept(
        &mut self,
        o: Observation,
        received_ms: i64,
        historical: bool,
    ) -> Result<Option<Call>, &'static str> {
        match self.accept_validated(o, received_ms, historical) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.counts.rejected_observations += 1;
                Err(error)
            }
        }
    }
    fn accept_validated(
        &mut self,
        o: Observation,
        received_ms: i64,
        historical: bool,
    ) -> Result<Option<Call>, &'static str> {
        o.validate()?;
        self.has_private_context |= o.inspection.is_some();
        if self.sequence == u64::MAX || self.next_id == u64::MAX {
            return Err("Trace sequence exhausted");
        }
        let source = self.identity.label("source", (&o.source, &o.source_epoch));
        if self.sources.len() >= MAX_SOURCES && !self.sources.contains(&source) {
            return Err("Trace source limit reached");
        }
        let execution = self.identity.label("execution", (&source, &o.execution_id));
        let delivery = o
            .event_id
            .as_ref()
            .map(|id| self.identity.label("event", (&execution, id)));
        let signature = self.identity.label(
            "observation",
            (
                &o.call_id,
                &o.tool,
                &o.server,
                &o.plugin,
                o.phase,
                o.timestamp_ms,
                o.duration_ms,
                o.execution_success,
                o.tool_error,
            ),
        );
        if let Some(prior) = delivery.as_ref().and_then(|id| self.deliveries.get(id)) {
            if prior != &signature {
                return Err("Conflicting trace event identity");
            }
            self.counts.duplicate_observations += 1;
            return Ok(None);
        }
        let identity = o
            .call_id
            .as_ref()
            .map(|id| self.identity.label("call", (&execution, id)));
        let existing = identity
            .as_ref()
            .and_then(|id| self.by_identity.get(id))
            .copied();
        let id = existing.unwrap_or(self.next_id);
        if existing.is_none() {
            self.next_id += 1;
            self.counts.distinct_calls += 1;
            self.calls.insert(
                id,
                Tracked::new(id, source.clone(), execution, &o, received_ms, historical),
            );
            if let Some(key) = identity {
                self.by_identity.insert(key, id);
            }
        }
        self.sources.insert(source);
        if let Some(key) = delivery {
            self.delivery_order.push_back(key.clone());
            self.deliveries.insert(key, signature);
            if self.delivery_order.len() > MAX_DEDUP {
                if let Some(old) = self.delivery_order.pop_front() {
                    self.deliveries.remove(&old);
                }
            }
        }
        let tracked = self.calls.get_mut(&id).expect("inserted or retained call");
        let before = tracked.call.clone();
        self.counts.accepted_observations += 1;
        if !tracked.observe(&o) {
            self.counts.duplicate_observations += 1;
            self.enforce_limits();
            return Ok(None);
        }
        self.sequence += 1;
        tracked.call.revision = self.sequence;
        let updated = tracked.call.clone();
        if existing.is_some() {
            self.revisions.push_back(before);
        }
        self.revisions.push_back(updated.clone());
        self.enforce_limits();
        Ok(Some(updated))
    }

    /// Source termination is evidence of a gap, never a tool timeout.
    pub fn mark_incomplete(&mut self) -> Result<(), &'static str> {
        let ids: Vec<_> = self
            .calls
            .iter()
            .filter(|(_, t)| !t.call.completion_observed && !t.call.gap_observed)
            .map(|(&id, _)| id)
            .collect();
        for id in ids {
            self.mark_call_incomplete(id)?;
        }
        self.enforce_limits();
        Ok(())
    }
    fn mark_call_incomplete(&mut self, id: u64) -> Result<(), &'static str> {
        let next = self
            .sequence
            .checked_add(1)
            .ok_or("Trace sequence exhausted")?;
        let tracked = self.calls.get_mut(&id).ok_or("Trace call unavailable")?;
        let before = tracked.call.clone();
        if tracked.gap() {
            self.sequence = next;
            tracked.call.revision = next;
            self.revisions.push_back(before);
            self.revisions.push_back(tracked.call.clone());
            self.counts.incomplete_calls += 1;
        }
        Ok(())
    }
    fn enforce_limits(&mut self) {
        let pending: Vec<_> = self
            .calls
            .iter()
            .filter(|(_, t)| t.call.status == Status::Running)
            .map(|(&id, _)| id)
            .collect();
        for id in pending
            .iter()
            .take(pending.len().saturating_sub(MAX_PENDING))
        {
            let _ = self.mark_call_incomplete(*id);
        }
        while self.calls.len() > self.max_calls {
            if let Some((id, _)) = self.calls.pop_first() {
                self.by_identity.retain(|_, v| *v != id);
                self.floor = self.sequence;
                self.counts.evicted_calls += 1;
            }
        }
        while self.revisions.len() > self.max_revisions || self.charged_bytes() > MAX_CHARGED_BYTES
        {
            if self.revisions.pop_front().is_none() {
                break;
            }
            // Conservative invalidation: do not serve a partly reconstructed snapshot.
            self.floor = self.sequence;
            self.counts.evicted_revisions += 1;
        }
    }
    fn validate_watermark(&self, at: u64) -> Result<(), &'static str> {
        if at < self.floor {
            Err("Trace history expired")
        } else if at > self.sequence {
            Err("Invalid trace watermark")
        } else {
            Ok(())
        }
    }
    fn at(&self, id: u64, watermark: u64) -> Option<&Call> {
        let current = &self.calls.get(&id)?.call;
        if current.revision <= watermark {
            return Some(current);
        }
        self.revisions
            .iter()
            .rev()
            .filter(|c| c.id == id && c.revision <= watermark)
            .max_by_key(|c| c.revision)
    }
    pub fn detail(&self, id: u64, as_of: u64) -> Result<Call, &'static str> {
        self.validate_watermark(as_of)?;
        self.at(id, as_of).cloned().ok_or("Trace call unavailable")
    }
    pub fn page(
        &self,
        filter: &Filter,
        as_of: Option<u64>,
        cursor: Option<&str>,
    ) -> Result<Page, &'static str> {
        if [
            &filter.plugin,
            &filter.tool,
            &filter.execution,
            &filter.search,
        ]
        .into_iter()
        .any(|v| v.as_ref().is_some_and(|s| s.len() > 200))
            || filter
                .since_ms
                .is_some_and(|v| !(0..=253_402_300_799_999).contains(&v))
        {
            return Err("Invalid trace filter");
        }
        let fingerprint = self.identity.label("filter", filter);
        let (watermark, before) = if let Some(cursor) = cursor {
            if cursor.len() > 160 {
                return Err("Invalid trace cursor");
            }
            let parts: Vec<_> = cursor.split(':').collect();
            if parts.len() != 4 || parts[0] != self.epoch || parts[2] != fingerprint {
                return Err("Invalid trace cursor");
            }
            let at = parts[1]
                .parse::<u64>()
                .map_err(|_| "Invalid trace cursor")?;
            let id = parts[3]
                .parse::<u64>()
                .map_err(|_| "Invalid trace cursor")?;
            if as_of.is_some_and(|v| v != at) {
                return Err("Invalid trace cursor");
            }
            (at, id)
        } else {
            (as_of.unwrap_or(self.sequence), u64::MAX)
        };
        self.validate_watermark(watermark)?;
        let mut calls = Vec::new();
        let mut matched = 0;
        let mut bytes = 0;
        let mut full = false;
        for (&id, _) in self.calls.range(..before).rev() {
            let Some(call) = self.at(id, watermark) else {
                continue;
            };
            if !filter.matches(call) {
                continue;
            }
            matched += 1;
            let size = serde_json::to_vec(call)
                .map_err(|_| "Trace encoding failed")?
                .len()
                + 1;
            if !full && calls.len() < 100 && bytes + size <= 60 * 1024 {
                bytes += size;
                calls.push(call.clone());
            } else {
                full = true;
            }
        }
        let next_cursor = if matched > calls.len() {
            calls
                .last()
                .map(|c| format!("{}:{}:{}:{}", self.epoch, watermark, fingerprint, c.id))
        } else {
            None
        };
        Ok(Page {
            schema_version: SCHEMA_VERSION,
            collector_epoch: self.epoch.clone(),
            as_of: watermark,
            matched,
            calls,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(id: &str, phase: Phase) -> Observation {
        Observation {
            inspection: None,
            schema_version: 1,
            source: "PRIVATE_SOURCE".into(),
            source_epoch: "PRIVATE_EPOCH".into(),
            execution_id: "PRIVATE_EXECUTION".into(),
            call_id: Some(id.into()),
            event_id: None,
            tool: "docs.search".into(),
            server: Some("codex_apps".into()),
            plugin: Some("example".into()),
            phase,
            timestamp_ms: Some(if phase == Phase::Started { 100 } else { 200 }),
            duration_ms: None,
            execution_success: None,
            tool_error: None,
        }
    }
    #[test]
    fn scope_isolation_duplicate_delivery_and_missing_ids() {
        let mut s = Store::default();
        let mut o = observation("PRIVATE_CALL", Phase::Started);
        let a = s.accept(o.clone(), 100, false).unwrap().unwrap();
        assert!(s.accept(o.clone(), 110, false).unwrap().is_none());
        o.source_epoch = "other".into();
        assert_ne!(s.accept(o.clone(), 110, false).unwrap().unwrap().id, a.id);
        o.call_id = None;
        let a = s.accept(o.clone(), 110, false).unwrap().unwrap();
        let b = s.accept(o, 110, false).unwrap().unwrap();
        assert_ne!(a.id, b.id);
        assert!(!a.correlated);
        assert!(
            !serde_json::to_string(&s.page(&Filter::default(), None, None).unwrap())
                .unwrap()
                .contains("PRIVATE")
        );
    }
    #[test]
    fn large_counters_are_exact_and_query_work_is_bounded() {
        let counts = Counts {
            distinct_calls: u64::MAX,
            ..Counts::default()
        };
        assert_eq!(
            serde_json::to_value(counts).unwrap()["distinct_calls"],
            u64::MAX.to_string()
        );
        let filter = Filter {
            search: Some("x".repeat(201)),
            ..Filter::default()
        };
        assert_eq!(
            Store::default().page(&filter, None, None).err(),
            Some("Invalid trace filter")
        );
    }
    #[test]
    fn event_id_deduplication_is_scoped_and_conflicts_are_rejected() {
        let mut s = Store::default();
        let mut o = observation("a", Phase::Completed);
        o.event_id = Some("PRIVATE_EVENT".into());
        s.accept(o.clone(), 200, false).unwrap();
        assert!(s.accept(o.clone(), 210, false).unwrap().is_none());
        o.tool_error = Some(true);
        assert_eq!(
            s.accept(o.clone(), 220, false).err(),
            Some("Conflicting trace event identity")
        );
        o.execution_id = "other".into();
        assert!(s.accept(o, 230, false).unwrap().is_some());
    }
    #[test]
    fn pause_freezes_updates_details_filters_and_pagination() {
        let mut s = Store::default();
        for n in 0..130 {
            s.accept(observation(&n.to_string(), Phase::Started), 100, false)
                .unwrap();
        }
        let frozen = s.snapshot().watermark;
        let first = s.page(&Filter::default(), Some(frozen), None).unwrap();
        let first_ids: Vec<_> = first.calls.iter().map(|c| c.id).collect();
        for n in 0..130 {
            s.accept(observation(&n.to_string(), Phase::Completed), 200, false)
                .unwrap();
        }
        let call = s.detail(first_ids[0], frozen).unwrap();
        assert_eq!(call.status, Status::Running);
        assert_eq!(
            s.detail(first_ids[0], s.sequence).unwrap().status,
            Status::Completed
        );
        let rest = s
            .page(&Filter::default(), None, first.next_cursor.as_deref())
            .unwrap();
        assert_eq!(first.calls.len() + rest.calls.len(), 130);
        assert!(
            rest.calls
                .iter()
                .all(|c| c.status == Status::Running && !first_ids.contains(&c.id))
        );
        let filter = Filter {
            status: Some(Status::Completed),
            ..Filter::default()
        };
        assert_eq!(s.page(&filter, Some(frozen), None).unwrap().matched, 0);
        assert!(s.page(&filter, None, first.next_cursor.as_deref()).is_err());
        assert!(
            Store::default()
                .page(&Filter::default(), None, first.next_cursor.as_deref())
                .is_err()
        );
    }
    #[test]
    fn eviction_expires_old_views_and_never_returns_current_as_old() {
        let mut s = Store {
            max_calls: 2,
            max_revisions: 3,
            ..Store::default()
        };
        s.accept(observation("a", Phase::Started), 100, false)
            .unwrap();
        let old = s.sequence;
        for name in ["b", "c", "d"] {
            s.accept(observation(name, Phase::Started), 100, false)
                .unwrap();
        }
        assert_eq!(
            s.page(&Filter::default(), Some(old), None).err(),
            Some("Trace history expired")
        );
        assert_eq!(s.calls.len(), 2);
        assert!(s.revisions.len() <= 3);
        let frozen = s.sequence;
        let id = s.calls.last_key_value().unwrap().0.to_owned();
        s.accept(observation("d", Phase::Completed), 200, false)
            .unwrap();
        assert_eq!(s.detail(id, frozen).err(), Some("Trace history expired"));
        assert_eq!(s.detail(id, s.sequence).unwrap().status, Status::Completed);
    }
    #[test]
    fn pending_and_total_retention_stay_bounded_under_flood() {
        let mut s = Store::default();
        for n in 0..2500 {
            s.accept(observation(&n.to_string(), Phase::Started), 100, false)
                .unwrap();
        }
        assert!(s.calls.len() <= MAX_CALLS);
        assert!(s.revisions.len() <= MAX_REVISIONS);
        assert!(s.charged_bytes() <= MAX_CHARGED_BYTES);
        assert!(
            s.calls
                .values()
                .filter(|c| c.call.status == Status::Running)
                .count()
                <= MAX_PENDING
        );
        assert!(s.counts.incomplete_calls > 0);
        s.mark_incomplete().unwrap();
        assert!(!s.calls.values().any(|c| c.call.status == Status::Running));
    }
}

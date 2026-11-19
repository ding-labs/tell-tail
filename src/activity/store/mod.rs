//! Bounded current items and revision history. Expired snapshots fail explicitly.
use super::model::*;
use crate::tool_trace::identity::Identity;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

pub mod query;
pub mod sources;
pub use sources::{Sources, TraceSource};

const MAX_ITEMS_PER_CATEGORY: usize = 1000;
const MAX_DIAGNOSTICS: usize = 256;
const MAX_REVISIONS: usize = 8000;
const MAX_SOURCES_HISTORY: usize = 128;
const MAX_BYTES: usize = 24 * 1024 * 1024;

#[derive(Clone)]
struct Entry {
    item: Item,
    bytes: usize,
}
struct SourceRevision {
    sequence: u64,
    value: Sources,
    bytes: usize,
}

pub struct Store {
    identity: Identity,
    pub epoch: String,
    pub sequence: u64,
    pub floor: u64,
    next_id: u64,
    current: BTreeMap<u64, Entry>,
    origins: BTreeMap<String, u64>,
    revisions: VecDeque<Entry>,
    sources: VecDeque<SourceRevision>,
    charged: usize,
    evicted_items: u64,
    evicted_revisions: u64,
    rejected: u64,
    max_category: usize,
    max_revisions: usize,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub collector_epoch: String,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub watermark: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub oldest_watermark: u64,
    pub observed_ms: i64,
    pub retained_items: usize,
    pub retained_revisions: usize,
    pub charged_bytes: usize,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub evicted_items: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub evicted_revisions: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub rejected: u64,
    pub sources: Sources,
}

impl Default for Store {
    fn default() -> Self {
        let identity = Identity::default();
        let epoch = identity.label("activity-epoch", 0);
        Self {
            identity,
            epoch,
            sequence: 0,
            floor: 0,
            next_id: 1,
            current: BTreeMap::new(),
            origins: BTreeMap::new(),
            revisions: VecDeque::new(),
            sources: VecDeque::new(),
            charged: 0,
            evicted_items: 0,
            evicted_revisions: 0,
            rejected: 0,
            max_category: MAX_ITEMS_PER_CATEGORY,
            max_revisions: MAX_REVISIONS,
        }
    }
}
impl Store {
    pub fn filter_key(&self, filter: impl std::hash::Hash) -> String {
        self.identity.label("activity-filter", filter)
    }
    fn next(&mut self) -> Result<u64, &'static str> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Activity sequence exhausted")?;
        Ok(self.sequence)
    }
    /// The origin is a local adapter identity (file generation/offset, MCP ID or
    /// call ID), never a raw native chat/request ID. Revisions retain first ordering.
    pub fn accept(&mut self, origin: String, mut item: Item) -> Result<u64, &'static str> {
        let size = serde_json::to_vec(&item)
            .map_err(|_| "Invalid activity item")?
            .len();
        if origin.len() > 128 || size > MAX_ITEM_BYTES {
            self.rejected += 1;
            return Err("Activity item exceeds metadata limit");
        }
        let prior = self
            .origins
            .get(&origin)
            .and_then(|id| self.current.get(id));
        if let Some(prior) = prior {
            item.id = prior.item.id;
            item.ordinal = prior.item.ordinal;
            item.occurred_ms = prior.item.occurred_ms;
            item.revision = prior.item.revision;
            if item == prior.item {
                return Ok(item.id);
            }
        } else {
            item.id = self.next_id;
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or("Activity identity exhausted")?;
            item.ordinal = self
                .sequence
                .checked_add(1)
                .ok_or("Activity sequence exhausted")?;
        }
        item.revision = self.next()?;
        let id = item.id;
        let category = item.category;
        let diagnostic = item.diagnostic;
        // Conservative charge covers map nodes, keys, string capacity and Vec overhead.
        let entry = Entry {
            item,
            bytes: size + 1024,
        };
        if let Some(old) = self.current.insert(id, entry.clone()) {
            self.charged -= old.bytes;
        }
        self.origins.insert(origin, id);
        self.charged += entry.bytes * 2;
        self.revisions.push_back(entry);
        while self
            .current
            .values()
            .filter(|e| e.item.category == category)
            .count()
            > self.max_category
        {
            let oldest = self
                .current
                .iter()
                .find(|(_, e)| e.item.category == category)
                .map(|(&id, _)| id)
                .unwrap();
            self.evict_item(oldest);
        }
        if diagnostic {
            while self.current.values().filter(|e| e.item.diagnostic).count() > MAX_DIAGNOSTICS {
                let oldest = self
                    .current
                    .iter()
                    .find(|(_, e)| e.item.diagnostic)
                    .map(|(&id, _)| id)
                    .unwrap();
                self.evict_item(oldest);
            }
        }
        self.enforce();
        Ok(id)
    }
    fn evict_item(&mut self, id: u64) {
        if let Some(entry) = self.current.remove(&id) {
            self.charged -= entry.bytes;
            self.origins.retain(|_, value| *value != id);
            self.evicted_items += 1;
            self.floor = self.sequence;
        }
    }
    fn enforce(&mut self) {
        while self.revisions.len() > self.max_revisions || self.charged > MAX_BYTES {
            if let Some(old) = self.revisions.pop_front() {
                self.charged -= old.bytes;
                self.evicted_revisions += 1;
                self.floor = self.sequence;
            } else if let Some(id) = self.current.keys().next().copied() {
                self.evict_item(id);
            } else {
                break;
            }
        }
    }
    pub fn update_sources(&mut self, value: Sources) -> Result<(), &'static str> {
        if self.sources.back().is_some_and(|v| v.value == value) {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&value)
            .map_err(|_| "Invalid source status")?
            .len()
            + 1024;
        if bytes > 32 * 1024 {
            return Err("Source status exceeds metadata limit");
        }
        let sequence = self.next()?;
        self.sources.push_back(SourceRevision {
            sequence,
            value,
            bytes,
        });
        self.charged += bytes;
        while self.sources.len() > MAX_SOURCES_HISTORY {
            let old = self.sources.pop_front().unwrap();
            self.charged -= old.bytes;
            self.floor = self.sequence;
        }
        self.enforce();
        Ok(())
    }
    pub fn check(&self, as_of: u64) -> Result<(), &'static str> {
        if as_of < self.floor {
            Err("Activity history expired; return to live")
        } else if as_of > self.sequence {
            Err("Activity watermark is in the future")
        } else {
            Ok(())
        }
    }
    pub fn view(&self, as_of: u64) -> Result<Vec<&Item>, &'static str> {
        self.check(as_of)?;
        let mut items: BTreeMap<_, _> = self
            .current
            .iter()
            .filter(|(_, e)| e.item.revision <= as_of)
            .map(|(&id, e)| (id, &e.item))
            .collect();
        if as_of != self.sequence {
            for e in &self.revisions {
                if self.current.contains_key(&e.item.id)
                    && e.item.revision <= as_of
                    && items
                        .get(&e.item.id)
                        .is_none_or(|prior| prior.revision < e.item.revision)
                {
                    items.insert(e.item.id, &e.item);
                }
            }
        }
        let mut items: Vec<_> = items.into_values().collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.ordinal));
        Ok(items)
    }
    pub fn detail(&self, id: u64, as_of: u64) -> Result<Item, &'static str> {
        self.view(as_of)?
            .into_iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or("Activity item unavailable at this watermark")
    }
    pub fn sources_at(&self, as_of: u64) -> Result<Sources, &'static str> {
        self.check(as_of)?;
        Ok(self
            .sources
            .iter()
            .rev()
            .find(|s| s.sequence <= as_of)
            .map(|s| s.value.clone())
            .unwrap_or_default())
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            schema_version: SCHEMA_VERSION,
            collector_epoch: self.epoch.clone(),
            watermark: self.sequence,
            oldest_watermark: self.floor,
            observed_ms: crate::mcp::time::now_ms(),
            retained_items: self.current.len(),
            retained_revisions: self.revisions.len(),
            charged_bytes: self.charged,
            evicted_items: self.evicted_items,
            evicted_revisions: self.evicted_revisions,
            rejected: self.rejected,
            sources: self
                .sources
                .back()
                .map(|s| s.value.clone())
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(category: Category) -> Item {
        Item::observation(category, "fixture", "Example")
    }
    #[test]
    fn freeze_preserves_revisions_order_and_source_state() {
        let mut s = Store::default();
        let id = s.accept("call-1".into(), item(Category::Tools)).unwrap();
        s.update_sources(Sources {
            running: true,
            ..Sources::default()
        })
        .unwrap();
        let frozen = s.sequence;
        let mut completed = item(Category::Tools);
        completed.outcome = "completed";
        completed.occurred_ms = Some(90);
        s.accept("call-1".into(), completed).unwrap();
        s.update_sources(Sources::default()).unwrap();
        assert_eq!(s.detail(id, frozen).unwrap().outcome, "observed");
        assert_eq!(s.detail(id, s.sequence).unwrap().outcome, "completed");
        assert_eq!(s.detail(id, s.sequence).unwrap().occurred_ms, None);
        assert_eq!(s.view(s.sequence).unwrap().len(), 1);
        assert!(s.sources_at(frozen).unwrap().running);
        assert!(!s.sources_at(s.sequence).unwrap().running);
    }
    #[test]
    fn noisy_category_does_not_evict_quiet_tools_and_expiry_is_explicit() {
        let mut s = Store {
            max_category: 4,
            max_revisions: 10,
            ..Store::default()
        };
        let tool = s.accept("tool".into(), item(Category::Tools)).unwrap();
        for i in 0..100 {
            s.accept(format!("file-{i}"), item(Category::Desktop))
                .unwrap();
        }
        assert!(s.detail(tool, s.sequence).is_ok());
        assert_eq!(s.current.len(), 5);
        assert!(s.view(1).unwrap_err().contains("expired"));
        assert!(s.revisions.len() <= 10);
        assert!(s.charged < MAX_BYTES);
        let mut large = item(Category::Desktop);
        large.summary = "x".repeat(MAX_ITEM_BYTES);
        assert!(s.accept("large".into(), large).is_err());
    }
}

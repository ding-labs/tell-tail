use super::{super::model::*, Store};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum View {
    #[default]
    Activity,
    Tools,
    Problems,
}
#[derive(Clone, Default, Hash)]
pub struct Filter {
    pub view: View,
    pub category: Option<Category>,
    pub search: String,
    pub plugin: Option<String>,
    pub server: Option<String>,
    pub source: Option<String>,
    pub client: Option<String>,
    pub scope: Option<String>,
    pub outcome: Option<String>,
    pub historical: Option<bool>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub diagnostics: bool,
}
impl Filter {
    pub fn matches(&self, i: &Item) -> bool {
        (self.view != View::Tools || matches!(i.category, Category::Tools | Category::Mcp))
            && (self.view != View::Problems || i.severity != Severity::Info)
            && self.category.is_none_or(|c| c == i.category)
            && (self.diagnostics || !i.diagnostic)
            && self
                .plugin
                .as_ref()
                .is_none_or(|s| i.plugin.as_ref() == Some(s))
            && self
                .server
                .as_ref()
                .is_none_or(|s| i.server.as_ref() == Some(s))
            && self.source.as_ref().is_none_or(|s| i.source == s)
            && self.client.as_ref().is_none_or(|s| i.client == s)
            && self
                .scope
                .as_ref()
                .is_none_or(|s| i.scope_ref.as_ref() == Some(s))
            && self.outcome.as_ref().is_none_or(|s| i.outcome == s)
            && self.historical.is_none_or(|v| i.historical == v)
            && self
                .since_ms
                .is_none_or(|t| i.occurred_ms.is_some_and(|v| v >= t))
            && self
                .until_ms
                .is_none_or(|t| i.occurred_ms.is_some_and(|v| v < t))
            && (self.search.is_empty()
                || format!(
                    "{} {} {} {} {} {} {} {}",
                    i.inspection
                        .as_ref()
                        .and_then(|c| c.chat.as_deref())
                        .unwrap_or(""),
                    i.inspection
                        .as_ref()
                        .and_then(|c| c.operation.as_deref())
                        .unwrap_or(""),
                    i.summary,
                    i.subject,
                    i.plugin.as_deref().unwrap_or(""),
                    i.server.as_deref().unwrap_or(""),
                    i.method.unwrap_or(""),
                    i.scope_ref.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(&self.search.to_lowercase()))
    }
}

#[derive(Serialize)]
pub struct Row {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inspection: Option<std::sync::Arc<crate::tool_trace::inspection::Inspection>>,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub id: u64,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub revision: u64,
    pub category: Category,
    pub severity: Severity,
    pub summary: String,
    pub subject: String,
    pub plugin: Option<String>,
    pub server: Option<String>,
    pub client: &'static str,
    pub source: &'static str,
    pub scope: &'static str,
    pub scope_ref: Option<String>,
    pub occurred_ms: Option<i64>,
    pub received_ms: i64,
    pub historical: bool,
    pub diagnostic: bool,
    pub outcome: &'static str,
    pub duration_ms: Option<u64>,
}
impl From<&Item> for Row {
    fn from(i: &Item) -> Self {
        Self {
            inspection: i.inspection.clone(),
            id: i.id,
            revision: i.revision,
            category: i.category,
            severity: i.severity,
            summary: i.summary.clone(),
            subject: i.subject.clone(),
            plugin: i.plugin.clone(),
            server: i.server.clone(),
            client: i.client,
            source: i.source,
            scope: i.scope,
            scope_ref: i.scope_ref.clone(),
            occurred_ms: i.occurred_ms,
            received_ms: i.received_ms,
            historical: i.historical,
            diagnostic: i.diagnostic,
            outcome: i.outcome,
            duration_ms: i.duration_ms,
        }
    }
}
#[derive(Serialize)]
pub struct Facets {
    pub plugins: Vec<String>,
    pub servers: Vec<String>,
}
#[derive(Serialize)]
pub struct Page {
    pub schema_version: u32,
    pub collector_epoch: String,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub as_of: u64,
    pub matched: usize,
    pub diagnostics_hidden: usize,
    pub items: Vec<Row>,
    pub next_cursor: Option<String>,
    pub facets: Facets,
}
#[derive(Serialize)]
pub struct Bins {
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub as_of: u64,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub unknown_time: usize,
    pub matched: usize,
    pub lanes: Vec<Lane>,
}
#[derive(Serialize)]
pub struct Lane {
    pub category: Category,
    pub counts: Vec<usize>,
    pub failures: Vec<usize>,
}
#[derive(Serialize)]
pub struct Problem {
    pub item: Row,
    pub id: String,
    #[serde(serialize_with = "crate::tool_trace::model::decimal")]
    pub item_id: u64,
    pub summary: String,
    pub subject: String,
    pub observations: usize,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub recovered_ms: Option<i64>,
    pub scope: &'static str,
    pub advice: Option<&'static str>,
}

impl Store {
    pub fn page(
        &self,
        filter: &Filter,
        as_of: u64,
        cursor: Option<&str>,
    ) -> Result<Page, &'static str> {
        let key = self.filter_key(filter);
        let before = if let Some(cursor) = cursor {
            let parts: Vec<_> = cursor.split('.').collect();
            if parts.len() != 4
                || parts[0] != self.epoch
                || parts[1] != as_of.to_string()
                || parts[3] != key
            {
                return Err("Cursor does not match this activity view");
            }
            Some(
                parts[2]
                    .parse::<u64>()
                    .map_err(|_| "Invalid activity cursor")?,
            )
        } else {
            None
        };
        let all = self.view(as_of)?;
        let facets = Facets {
            plugins: all
                .iter()
                .filter_map(|i| i.plugin.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(128)
                .collect(),
            servers: all
                .iter()
                .filter_map(|i| i.server.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(128)
                .collect(),
        };
        let matches: Vec<_> = all.iter().copied().filter(|i| filter.matches(i)).collect();
        let diagnostics = Filter {
            diagnostics: true,
            ..filter.clone()
        };
        let diagnostics_hidden = if filter.diagnostics {
            0
        } else {
            all.iter()
                .filter(|i| i.diagnostic && diagnostics.matches(i))
                .count()
        };
        let mut rows = Vec::new();
        let mut bytes = 0;
        let mut last = None;
        let mut more = false;
        for i in matches
            .iter()
            .filter(|i| before.is_none_or(|b| i.ordinal < b))
        {
            let row = Row::from(*i);
            let size = serde_json::to_vec(&row)
                .map_err(|_| "Invalid activity row")?
                .len();
            if rows.len() >= 100 || bytes + size > 48 * 1024 {
                more = true;
                break;
            }
            bytes += size;
            rows.push(row);
            last = Some(i.ordinal);
        }
        Ok(Page {
            schema_version: SCHEMA_VERSION,
            collector_epoch: self.epoch.clone(),
            as_of,
            matched: matches.len(),
            diagnostics_hidden,
            items: rows,
            next_cursor: last
                .filter(|_| more)
                .map(|last| format!("{}.{as_of}.{last}.{key}", self.epoch)),
            facets,
        })
    }
    pub fn bins(&self, filter: &Filter, as_of: u64, buckets: usize) -> Result<Bins, &'static str> {
        if !(1..=120).contains(&buckets) {
            return Err("Invalid activity bucket count");
        }
        let items: Vec<_> = self
            .view(as_of)?
            .into_iter()
            .filter(|i| filter.matches(i))
            .collect();
        let start = filter
            .since_ms
            .or_else(|| items.iter().filter_map(|i| i.occurred_ms).min());
        let end = filter.until_ms.or_else(|| {
            items
                .iter()
                .filter_map(|i| i.occurred_ms)
                .max()
                .and_then(|v| v.checked_add(1))
        });
        let mut lanes: Vec<_> = [
            Category::Tools,
            Category::Mcp,
            Category::Connections,
            Category::Desktop,
        ]
        .into_iter()
        .map(|category| Lane {
            category,
            counts: vec![0; buckets],
            failures: vec![0; buckets],
        })
        .collect();
        if let (Some(start), Some(end)) = (start, end) {
            let span = (i128::from(end) - i128::from(start)).max(1);
            for item in &items {
                if let Some(t) = item.occurred_ms {
                    let index = (((i128::from(t) - i128::from(start)).max(0) * buckets as i128)
                        / span)
                        .min((buckets - 1) as i128) as usize;
                    let lane = lanes
                        .iter_mut()
                        .find(|l| l.category == item.category)
                        .unwrap();
                    lane.counts[index] += 1;
                    lane.failures[index] += usize::from(item.severity == Severity::Error);
                }
            }
        }
        Ok(Bins {
            as_of,
            start_ms: start,
            end_ms: end,
            unknown_time: items.iter().filter(|i| i.occurred_ms.is_none()).count(),
            matched: items.len(),
            lanes,
        })
    }
    pub fn problems(&self, filter: &Filter, as_of: u64) -> Result<Vec<Problem>, &'static str> {
        let all = self.view(as_of)?;
        let mut groups: BTreeMap<String, Problem> = BTreeMap::new();
        for item in all
            .iter()
            .copied()
            .filter(|i| i.severity != Severity::Info && filter.matches(i))
        {
            let scope = item
                .scope_ref
                .clone()
                .unwrap_or_else(|| format!("unscoped-{}", item.id));
            let key =
                self.filter_key((item.client, &item.subject, item.kind, &item.summary, scope));
            if let Some(group) = groups.get_mut(&key) {
                group.observations += 1;
                group.first_ms = group.first_ms.into_iter().chain(item.occurred_ms).min();
                group.last_ms = group.last_ms.into_iter().chain(item.occurred_ms).max();
            } else {
                groups.insert(
                    key.clone(),
                    Problem {
                        item: Row::from(item),
                        id: key,
                        item_id: item.id,
                        summary: item.summary.clone(),
                        subject: item.subject.clone(),
                        observations: 1,
                        first_ms: item.occurred_ms,
                        last_ms: item.occurred_ms,
                        recovered_ms: None,
                        scope: item.scope,
                        advice: item.advice,
                    },
                );
            }
        }
        for group in groups.values_mut() {
            let Some(failure) = all.iter().find(|i| i.id == group.item_id) else {
                continue;
            };
            if matches!(failure.kind, "mcp_startup_failed" | "mcp_unexpected_close")
                && failure.scope_ref.is_some()
                && group.last_ms.is_some()
            {
                group.recovered_ms = all
                    .iter()
                    .filter(|i| {
                        i.kind == "mcp_ready"
                            && i.client == failure.client
                            && i.subject == failure.subject
                            && i.scope_ref == failure.scope_ref
                    })
                    .filter_map(|i| i.occurred_ms)
                    .filter(|t| Some(*t) >= group.last_ms)
                    .max();
            }
        }
        let mut groups: Vec<_> = groups.into_values().collect();
        groups.sort_by_key(|g| std::cmp::Reverse(g.item_id));
        Ok(groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_bins_and_paging_share_a_frozen_view() {
        let mut s = Store::default();
        for n in 0..205 {
            let mut item = Item::observation(Category::Desktop, "test", "Test event");
            item.occurred_ms = Some(n);
            item.diagnostic = n % 2 == 0;
            s.accept(n.to_string(), item).unwrap();
        }
        let f = Filter::default();
        let frozen = s.sequence;
        let first = s.page(&f, frozen, None).unwrap();
        assert_eq!(first.matched, 102);
        assert_eq!(first.diagnostics_hidden, 103);
        let second = s.page(&f, frozen, first.next_cursor.as_deref()).unwrap();
        assert_eq!(first.items.len() + second.items.len(), 102);
        assert!(
            first
                .items
                .iter()
                .all(|a| second.items.iter().all(|b| a.id != b.id))
        );
        let bins = s.bins(&f, frozen, 60).unwrap();
        assert_eq!(
            bins.lanes.iter().flat_map(|l| &l.counts).sum::<usize>(),
            102
        );
        let changed = Filter {
            search: "other".into(),
            ..f.clone()
        };
        assert!(
            s.page(&changed, frozen, first.next_cursor.as_deref())
                .is_err()
        );
        assert!(
            s.page(&f, frozen - 1, first.next_cursor.as_deref())
                .is_err()
        );
    }
    #[test]
    fn failure_groups_count_all_retained_evidence_before_response_truncation() {
        let mut s = Store::default();
        for n in 0..1100 {
            let category = if n % 2 == 0 {
                Category::Tools
            } else {
                Category::Mcp
            };
            let mut item = Item::observation(category, "failure", "Failed");
            item.severity = Severity::Error;
            s.accept(n.to_string(), item).unwrap();
        }
        assert_eq!(
            s.problems(&Filter::default(), s.sequence).unwrap().len(),
            1100
        );
    }
    #[test]
    fn recovery_requires_matching_scope_and_affirmative_evidence() {
        let mut s = Store::default();
        let mut failure = Item::observation(Category::Mcp, "mcp_startup_failed", "Startup failed");
        failure.severity = Severity::Error;
        failure.scope_ref = Some("scope-a".into());
        failure.occurred_ms = Some(10);
        s.accept("failed".into(), failure).unwrap();
        let frozen = s.sequence;
        let mut ready = Item::observation(Category::Mcp, "mcp_ready", "Ready");
        ready.scope_ref = Some("scope-b".into());
        ready.occurred_ms = Some(20);
        s.accept("unrelated".into(), ready.clone()).unwrap();
        assert_eq!(
            s.problems(&Filter::default(), s.sequence).unwrap()[0].recovered_ms,
            None
        );
        ready.scope_ref = Some("scope-a".into());
        s.accept("related".into(), ready).unwrap();
        assert_eq!(
            s.problems(&Filter::default(), s.sequence).unwrap()[0].recovered_ms,
            Some(20)
        );
        assert_eq!(
            s.problems(&Filter::default(), frozen).unwrap()[0].recovered_ms,
            None
        );
    }
}

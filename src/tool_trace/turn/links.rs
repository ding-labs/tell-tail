//! The dispatch event and owning execute span may arrive in either order.
use super::{Locator, Tool};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub chat: String,
    pub turn: String,
    pub call: String,
    pub cell: String,
    // Only code_mode.handler.execute owns a cell. A wait on that cell never owns it.
    pub child_tool: Option<String>,
}

#[derive(Default)]
pub struct Links(VecDeque<Link>);

#[derive(Default)]
pub struct Scope {
    pub turn: Option<String>,
    pub conflict: bool,
    pub partial: bool,
    pub tools: Vec<(String, Tool)>,
    pub children: Vec<(String, Tool)>,
}

impl Links {
    pub fn push(&mut self, link: Link) {
        if self.0.len() == 8192 {
            self.0.pop_front();
        }
        self.0.push_back(link);
    }

    pub fn resolve(&self, loc: &Locator, live: &HashMap<String, Tool>) -> Scope {
        let mut scope = Scope::default();
        let mut matches = self
            .0
            .iter()
            .filter(|r| r.chat == loc.chat && r.call == loc.call);
        let Some(selected) = matches.next() else {
            return scope;
        };
        if matches.any(|r| r != selected) {
            scope.conflict = true;
            return scope;
        }
        scope.turn = Some(selected.turn.clone());
        let in_turn = || {
            self.0
                .iter()
                .filter(|r| r.chat == loc.chat && r.turn == selected.turn)
        };
        let own_cell = selected.child_tool.is_none()
            && !in_turn().any(|r| {
                r.cell == selected.cell && r.child_tool.is_none() && r.call != selected.call
            });
        let mut by_call = HashMap::new();
        let mut ambiguous = HashSet::new();
        for link in self.0.iter().filter(|r| r.chat == loc.chat) {
            if by_call
                .insert(&link.call, link)
                .is_some_and(|old| old != link)
            {
                ambiguous.insert(&link.call);
            }
        }
        let mut seen = HashSet::new();
        for link in in_turn().rev() {
            if !seen.insert(&link.call) {
                continue;
            }
            if ambiguous.contains(&link.call) {
                continue;
            }
            let name = link.child_tool.as_deref().unwrap_or("functions.exec");
            let mut tool = live
                .get(&link.call)
                .filter(|t| t.name == name)
                .cloned()
                .unwrap_or_else(|| Tool {
                    name: name.into(),
                    operation: None,
                    outcome: "observed",
                    exit_code: None,
                    selected: false,
                    output_excerpt: None,
                });
            tool.selected = link.call == loc.call;
            if own_cell && link.cell == selected.cell && link.child_tool.is_some() {
                if scope.children.len() < super::MAX_TOOLS {
                    scope.children.push((link.call.clone(), tool.clone()));
                } else {
                    scope.partial = true;
                }
            }
            if scope.tools.len() < super::MAX_TOOLS || tool.selected {
                scope.tools.push((link.call.clone(), tool));
            } else {
                scope.partial = true;
            }
        }
        scope.tools.reverse();
        scope.children.reverse();
        scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interleaved_wrappers_share_only_exact_dispatches() {
        let loc = Locator::new("01900000-0000-7000-b000-000000000001", "a").unwrap();
        let make = |turn: &str, call: &str, cell: &str, tool: Option<&str>| Link {
            chat: loc.chat.clone(),
            turn: turn.into(),
            call: super::super::tag(call),
            cell: cell.into(),
            child_tool: tool.map(Into::into),
        };
        let mut links = Links::default();
        for link in [
            make("turn", "child", "1", Some("functions.exec_command")),
            make("other", "earlier", "1", Some("old")),
            make("turn", "b", "2", None),
            make("turn", "sibling", "2", Some("other")),
            make("turn", "a", "1", None),
            make("turn", "child", "1", Some("functions.exec_command")),
        ] {
            links.push(link)
        }
        let scope = links.resolve(&loc, &HashMap::new());
        assert_eq!(scope.tools.len(), 4);
        assert_eq!(scope.children.len(), 1);
        assert_eq!(scope.children[0].1.name, "functions.exec_command");
        links.push(make("turn", "collision", "1", None));
        assert!(links.resolve(&loc, &HashMap::new()).children.is_empty());
        links.push(make("other", "a", "3", None));
        assert!(links.resolve(&loc, &HashMap::new()).conflict);
    }
}

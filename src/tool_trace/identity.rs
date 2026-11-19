use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hash},
};

/// Two independently keyed hashes keep public identifiers local to this run.
/// No source IDs or payloads are retained in a reverse lookup table.
#[derive(Default)]
pub struct Identity {
    first: RandomState,
    second: RandomState,
}
impl Identity {
    pub fn label(&self, domain: &str, value: impl Hash) -> String {
        format!(
            "{:016x}{:016x}",
            self.first.hash_one((domain, &value)),
            self.second.hash_one((domain, &value))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_are_scoped_and_change_between_runs() {
        let a = Identity::default();
        assert_eq!(a.label("call", "private"), a.label("call", "private"));
        assert_ne!(a.label("call", "private"), a.label("execution", "private"));
        assert_ne!(
            a.label("call", "private"),
            Identity::default().label("call", "private")
        );
    }
}

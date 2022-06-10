//! Contains the `Secrets` type.

use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// A container that holds secrets accessible from Vector / VRL.
#[derive(Clone, Default, PartialEq, PartialOrd)]
pub struct Secrets {
    secrets: BTreeMap<String, Arc<str>>,
}

impl Debug for Secrets {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for key in self.secrets.keys() {
            map.entry(key, &"<redacted secret>");
        }
        map.finish()
    }
}

impl Secrets {
    /// Creates a new empty secrets container
    #[must_use]
    pub fn new() -> Self {
        Self {
            secrets: BTreeMap::new(),
        }
    }

    /// Gets a secret
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Arc<str>> {
        self.secrets.get(key)
    }

    /// Inserts a new secret into the container.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Arc<str>>) {
        self.secrets.insert(key.into(), value.into());
    }

    /// Removes a secret
    pub fn remove(&mut self, key: &str) {
        self.secrets.remove(&key.to_owned());
    }

    /// Merged both together. If there are collisions, the value from `self` is kept.
    pub fn merge(&mut self, other: Self) {
        for (key, value) in other.secrets {
            self.secrets.entry(key).or_insert(value);
        }
    }
}

#[cfg(test)]
mod test {
    use crate::Secrets;

    #[test]
    fn test_merge() {
        let mut a = Secrets::new();
        a.insert("key-a", "value-a1");
        a.insert("key-b", "value-b1");

        let mut b = Secrets::new();
        b.insert("key-b", "value-b2");
        b.insert("key-c", "value-c2");

        a.merge(b);

        assert_eq!(a.get("key-a").unwrap().as_ref(), "value-a1");
        assert_eq!(a.get("key-b").unwrap().as_ref(), "value-b1");
        assert_eq!(a.get("key-c").unwrap().as_ref(), "value-c2");
    }
}

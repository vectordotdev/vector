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
    pub fn insert(&mut self, key: &str, value: impl Into<Arc<str>>) {
        self.secrets.insert(key.to_owned(), value.into());
    }

    /// Removes a secret
    pub fn remove(&mut self, key: &str) {
        self.secrets.remove(&key.to_owned());
    }
}

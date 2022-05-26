use crate::target::SecretTarget;
use std::collections::{BTreeMap, HashMap};
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
    pub fn new() -> Secrets {
        Secrets {
            secrets: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Arc<str>> {
        self.secrets.get(key)
    }

    pub fn insert(&mut self, key: &str, value: impl Into<Arc<str>>) {
        self.secrets.insert(key.to_owned(), value.into());
    }

    pub fn remove(&mut self, key: &str) {
        self.secrets.remove(&key.to_owned());
    }
}

impl SecretTarget for Secrets {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.get(key).map(|value| value.as_ref())
    }

    fn insert_secret(&mut self, key: &str, value: &str) {
        self.insert(key, value);
    }

    fn remove_secret(&mut self, key: &str) {
        self.remove(&key.to_owned());
    }
}

use crate::target::SecretTarget;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};

/// A container that holds secrets accessible from Vector / VRL.
pub struct Secrets {
    secrets: HashMap<String, String>,
}

impl Debug for Secrets {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for key in self.secrets.keys() {
            map.key(key).value(&"<redacted secret>");
        }
        map.finish()
    }
}

impl Secrets {
    pub fn new() -> Secrets {
        Secrets {
            secrets: HashMap::new(),
        }
    }
}

impl SecretTarget for Secrets {
    fn get_secret(&self, key: &str) -> Option<&str> {
        self.secrets.get(key).map(|value| value.as_str())
    }

    fn set_secret(&mut self, key: &str, value: &str) {
        self.secrets.insert(key.to_owned(), value.to_owned());
    }

    fn remove_secret(&mut self, key: &str) {
        self.secrets.remove(&key.to_owned());
    }
}

use std::fmt;

use serde::{Deserialize, Serialize};
pub use vector_core::config::ComponentKey;

// Unlike `ComponentKey`, we never deserialize these directly out of user configs, so it's fine to
// use the derive. They should really only be triggered by our hacky roundtrip-through-serde clone.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct OutputId {
    pub component: ComponentKey,
    pub port: Option<String>,
}

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.port {
            None => self.component.fmt(f),
            Some(port) => write!(f, "{}.{}", self.component, port),
        }
    }
}

impl From<ComponentKey> for OutputId {
    fn from(key: ComponentKey) -> Self {
        Self {
            component: key,
            port: None,
        }
    }
}

impl From<&ComponentKey> for OutputId {
    fn from(key: &ComponentKey) -> Self {
        Self::from(key.clone())
    }
}

impl From<(&ComponentKey, String)> for OutputId {
    fn from((key, name): (&ComponentKey, String)) -> Self {
        Self {
            component: key.clone(),
            port: Some(name),
        }
    }
}

// This panicking implementation is convenient for testing, but should never be enabled for use
// outside of tests.
#[cfg(test)]
impl From<&str> for OutputId {
    fn from(s: &str) -> Self {
        assert!(
            !s.contains('.'),
            "Cannot convert dotted paths to strings without more context"
        );
        let component = ComponentKey::from(s);
        component.into()
    }
}

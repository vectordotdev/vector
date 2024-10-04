use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
};

use vector_config::{configurable_component, ConfigurableString};

/// Component identifier.
#[configurable_component(no_deser, no_ser)]
#[derive(::serde::Deserialize, ::serde::Serialize)]
#[serde(from = "String", into = "String")]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ComponentKey {
    /// Component ID.
    id: String,
}

impl ComponentKey {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn join<D: fmt::Display>(&self, name: D) -> Self {
        Self {
            // ports and inner component use the same naming convention
            id: self.port(name),
        }
    }

    pub fn port<D: fmt::Display>(&self, name: D) -> String {
        format!("{}.{name}", self.id)
    }

    #[must_use]
    pub fn into_id(self) -> String {
        self.id
    }
}

impl From<String> for ComponentKey {
    fn from(id: String) -> Self {
        Self { id }
    }
}

impl From<&str> for ComponentKey {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<ComponentKey> for String {
    fn from(key: ComponentKey) -> Self {
        key.into_id()
    }
}

impl fmt::Display for ComponentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.id.fmt(f)
    }
}

impl Ord for ComponentKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for ComponentKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl ConfigurableString for ComponentKey {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_string() {
        let result: ComponentKey = serde_json::from_str("\"foo\"").unwrap();
        assert_eq!(result.id(), "foo");
    }

    #[test]
    fn serialize_string() {
        let item = ComponentKey::from("foo");
        let result = serde_json::to_string(&item).unwrap();
        assert_eq!(result, "\"foo\"");
    }

    #[test]
    #[allow(clippy::similar_names)]
    fn ordering() {
        let global_baz = ComponentKey::from("baz");
        let yolo_bar = ComponentKey::from("yolo.bar");
        let foo_bar = ComponentKey::from("foo.bar");
        let foo_baz = ComponentKey::from("foo.baz");
        let mut list = vec![&foo_baz, &yolo_bar, &global_baz, &foo_bar];
        list.sort();
        assert_eq!(list, vec![&global_baz, &foo_bar, &foo_baz, &yolo_bar]);
    }
}

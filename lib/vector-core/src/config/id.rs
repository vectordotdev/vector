use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
};

pub const GLOBAL_SCOPE: &str = "global";

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ComponentKey {
    id: String,
    scope_len: usize,
}

impl ComponentKey {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        if self.is_global() {
            &self.id
        } else {
            &self.id[(self.scope_len + 1)..]
        }
    }

    pub fn scope(&self) -> &str {
        if self.is_global() {
            GLOBAL_SCOPE
        } else {
            &self.id[0..self.scope_len]
        }
    }

    pub fn is_global(&self) -> bool {
        self.scope_len == 0
    }

    pub fn create_child(&self, name: &str) -> Self {
        Self {
            id: format!("{}.{}", self.id, name),
            scope_len: self.id.len(),
        }
    }
}

impl From<String> for ComponentKey {
    fn from(value: String) -> Self {
        let scope_len = value.rfind('.').unwrap_or(0);
        Self {
            id: value,
            scope_len,
        }
    }
}

impl From<&str> for ComponentKey {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl fmt::Display for ComponentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.id.fmt(f)
    }
}

impl Serialize for ComponentKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Ord for ComponentKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_scope = self.scope();
        let other_scope = other.scope();
        if self_scope == other_scope {
            self.id.cmp(&other.id)
        } else if self.is_global() {
            Ordering::Greater
        } else if other.is_global() {
            Ordering::Less
        } else {
            self_scope.cmp(other_scope)
        }
    }
}

impl PartialOrd for ComponentKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ComponentKeyVisitor;

impl<'de> Visitor<'de> for ComponentKeyVisitor {
    type Value = ComponentKey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ComponentKey::from(value))
    }
}

impl<'de> Deserialize<'de> for ComponentKey {
    fn deserialize<D>(deserializer: D) -> Result<ComponentKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(ComponentKeyVisitor)
    }
}

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

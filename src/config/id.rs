use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ComponentId {
    value: String,
    id: String,
}

impl ComponentId {
    pub fn global<T: Into<String>>(id: T) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            value: id,
        }
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
}
impl From<String> for ComponentId {
    fn from(value: String) -> Self {
        Self {
            id: value.clone(),
            value,
        }
    }
}

impl From<&str> for ComponentId {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl<T: ToString> From<&T> for ComponentId {
    fn from(value: &T) -> Self {
        Self {
            id: value.to_string(),
            value: value.to_string(),
        }
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl Serialize for ComponentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Ord for ComponentId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl PartialOrd for ComponentId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ComponentIdVisitor;

impl<'de> Visitor<'de> for ComponentIdVisitor {
    type Value = ComponentId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(ComponentId::from(value))
    }
}

impl<'de> Deserialize<'de> for ComponentId {
    fn deserialize<D>(deserializer: D) -> Result<ComponentId, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(ComponentIdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_string() {
        let result: ComponentId = serde_json::from_str("\"foo\"").unwrap();
        assert_eq!(result.id, "foo");
    }

    #[test]
    fn serialize_string() {
        let item = ComponentId::from("foo");
        let result = serde_json::to_string(&item).unwrap();
        assert_eq!(result, "\"foo\"");
    }
}

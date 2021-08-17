use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ComponentId {
    pub name: String,
}

impl From<String> for ComponentId {
    fn from(name: String) -> Self {
        Self { name }
    }
}

impl From<&str> for ComponentId {
    fn from(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl<T: ToString> From<&T> for ComponentId {
    fn from(value: &T) -> Self {
        Self {
            name: value.to_string(),
        }
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
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
        assert_eq!(result.name, "foo");
    }

    #[test]
    fn serialize_string() {
        let item = ComponentId::from("foo");
        let result = serde_json::to_string(&item).unwrap();
        assert_eq!(result, "\"foo\"");
    }
}

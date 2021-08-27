use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ComponentScope {
    Global,
    Pipeline(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ComponentId {
    value: String,
    id: String,
    scope: ComponentScope,
}

impl ComponentId {
    pub fn global<T: Into<String>>(id: T) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            value: id,
            scope: ComponentScope::Global,
        }
    }

    pub fn pipeline(pipeline: &str, id: &str) -> Self {
        let value = format!("{}#{}", pipeline, id);
        Self {
            id: id.to_string(),
            value,
            scope: ComponentScope::Pipeline(pipeline.to_string()),
        }
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn pipeline_str(&self) -> Option<&str> {
        match self.scope {
            ComponentScope::Pipeline(ref value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn into_pipeline(self, id: &str) -> Self {
        Self::pipeline(id, &self.id)
    }

    pub fn is_global(&self) -> bool {
        matches!(self.scope, ComponentScope::Global)
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
}

impl From<String> for ComponentId {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for ComponentId {
    fn from(value: &str) -> Self {
        let parts = value.split('#').take(2).collect::<Vec<_>>();
        if parts.len() == 2 {
            Self {
                id: parts[1].to_string(),
                value: value.to_string(),
                scope: ComponentScope::Pipeline(parts[0].to_string()),
            }
        } else {
            Self {
                id: value.to_string(),
                value: value.to_string(),
                scope: ComponentScope::Global,
            }
        }
    }
}

impl<T: ToString> From<&T> for ComponentId {
    fn from(value: &T) -> Self {
        Self::from(value.to_string())
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
        self.id.cmp(&other.id)
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

    #[test]
    fn from_pipeline() {
        let item = ComponentId::from("foo#bar");
        assert_eq!(item.id(), "bar");
        assert_eq!(item.scope, ComponentScope::Pipeline("foo".into()));
        assert_eq!(item.to_string(), "foo#bar");
    }
}

use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    cmp::{Ord, Ordering, PartialOrd},
    fmt,
};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ComponentScope {
    Global,
    Pipeline(String),
}

impl fmt::Display for ComponentScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::Pipeline(name) => write!(f, "pipeline:{}", name),
        }
    }
}

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

// TODO: this is too broad to be generally safe. Try to get rid of it.
impl<T: Into<ComponentKey>> From<T> for OutputId {
    fn from(key: T) -> Self {
        Self {
            component: key.into(),
            port: None,
        }
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

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ComponentKey {
    id: String,
    scope: ComponentScope,
}

impl ComponentKey {
    pub fn global<T: Into<String>>(id: T) -> Self {
        Self {
            id: id.into(),
            scope: ComponentScope::Global,
        }
    }

    pub fn pipeline(pipeline: &str, id: &str) -> Self {
        Self {
            id: id.to_string(),
            scope: ComponentScope::Pipeline(pipeline.to_string()),
        }
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub const fn scope(&self) -> &ComponentScope {
        &self.scope
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

    pub const fn is_global(&self) -> bool {
        matches!(self.scope, ComponentScope::Global)
    }
}

impl From<(Option<String>, String)> for ComponentKey {
    fn from(value: (Option<String>, String)) -> Self {
        if let Some(pipeline) = value.0 {
            Self {
                id: value.1,
                scope: ComponentScope::Pipeline(pipeline),
            }
        } else {
            Self {
                id: value.1,
                scope: ComponentScope::Global,
            }
        }
    }
}

impl From<String> for ComponentKey {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for ComponentKey {
    fn from(value: &str) -> Self {
        let parts = value.split('.').take(2).collect::<Vec<_>>();
        if parts.len() == 2 {
            Self {
                id: parts[1].to_string(),
                scope: ComponentScope::Pipeline(parts[0].to_string()),
            }
        } else {
            Self {
                id: value.to_string(),
                scope: ComponentScope::Global,
            }
        }
    }
}

impl<T: ToString> From<&T> for ComponentKey {
    fn from(value: &T) -> Self {
        Self::from(value.to_string())
    }
}

impl fmt::Display for ComponentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(pipeline) = self.pipeline_str() {
            write!(f, "{}.{}", pipeline, self.id)
        } else {
            self.id.fmt(f)
        }
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
        if self.scope == other.scope {
            self.id.cmp(&other.id)
        } else {
            self.scope.cmp(&other.scope)
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
        Ok(ComponentKey::global(value))
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
        assert_eq!(result.id, "foo");
    }

    #[test]
    fn serialize_string() {
        let item = ComponentKey::from("foo");
        let result = serde_json::to_string(&item).unwrap();
        assert_eq!(result, "\"foo\"");
    }

    #[test]
    fn from_pipeline() {
        let item = ComponentKey::from("foo.bar");
        assert_eq!(item.id(), "bar");
        assert_eq!(item.scope, ComponentScope::Pipeline("foo".into()));
        assert_eq!(item.to_string(), "foo.bar");
    }

    #[test]
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

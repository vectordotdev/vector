use std::fmt;

use vector_common::config::ComponentKey;

use super::configurable_component;
use crate::schema;

/// Component output identifier.
#[configurable_component]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OutputId {
    /// The component to which the output belongs.
    pub component: ComponentKey,

    /// The output port name, if not the default.
    pub port: Option<String>,
}

impl OutputId {
    /// Some situations, for example when validating a config file requires running the
    /// `transforms::output` function to retrieve the outputs, but we don't have an
    /// `OutputId` from a source. This gives us an `OutputId` that we can use.
    ///
    /// TODO: This is not a pleasant solution, but would require some significant refactoring
    /// to the topology code to avoid.
    pub fn dummy() -> Self {
        Self {
            component: "dummy".into(),
            port: None,
        }
    }

    /// Given a list of [`schema::Definition`]s, returns a [`Vec`] of tuples of
    /// this `OutputId` with each `Definition`.
    pub fn with_definitions(
        &self,
        definitions: impl IntoIterator<Item = schema::Definition>,
    ) -> Vec<(OutputId, schema::Definition)> {
        definitions
            .into_iter()
            .map(|definition| (self.clone(), definition))
            .collect()
    }
}

impl fmt::Display for OutputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.port {
            None => self.component.fmt(f),
            Some(port) => write!(f, "{}.{port}", self.component),
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

impl From<(String, Option<String>)> for OutputId {
    fn from((component, port): (String, Option<String>)) -> Self {
        Self {
            component: component.into(),
            port,
        }
    }
}

// This panicking implementation is convenient for testing, but should never be enabled for use
// outside of tests.
#[cfg(any(test, feature = "test"))]
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

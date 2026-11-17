use std::fmt;

use sha2::{Digest, Sha256};

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
    /// A deterministic, route-specific schema key. Multiple branches from one
    /// source must not alias when they reconverge downstream.
    #[must_use]
    pub fn schema_route(&self, output: &OutputId) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"vector-schema-route-v1");
        for id in [self, output] {
            let component = id.component.to_string();
            digest.update((component.len() as u64).to_be_bytes());
            digest.update(component.as_bytes());
            match &id.port {
                Some(port) => {
                    digest.update([1]);
                    digest.update((port.len() as u64).to_be_bytes());
                    digest.update(port.as_bytes());
                }
                None => digest.update([0]),
            }
        }
        let hash = digest.finalize();
        let mut name = String::from("_vector_schema_route_");
        for byte in hash {
            use std::fmt::Write;
            write!(&mut name, "{byte:02x}").expect("write to String");
        }
        Self {
            component: name.into(),
            port: None,
        }
    }

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

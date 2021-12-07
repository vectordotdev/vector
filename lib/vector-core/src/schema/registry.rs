use indexmap::IndexMap;

use crate::{config::ComponentKey, schema};

/// The schema registry is responsible for collecting all relevant details about the topology as it
/// relates to schema validation.
///
/// The registry is built _before_ any of the components are built, to allow passing in a list of
/// field purposes to the relevant components (e.g. the `remap` transform) for their respective
/// use-cases.
#[derive(Debug, Clone)]
pub struct Registry(State);

impl Default for Registry {
    fn default() -> Self {
        Self(State::Loading {
            sinks: IndexMap::default(),
            transforms: IndexMap::default(),
            sources: IndexMap::default(),
        })
    }
}

impl Registry {
    /// Finalize loading the registry.
    #[allow(clippy::unused_self)]
    pub fn finalize(self) -> Self {
        let sinks = match self.0 {
            State::Ok { .. } => return self,
            State::Loading { sinks, .. } => sinks,
        };

        let mut purposes = vec![];

        for (_, schema) in sinks {
            purposes.extend(&mut schema.purposes().iter().map(|v| v.as_str().to_owned()));
        }

        Self(State::Ok { purposes })
    }

    /// Check if the registry is still loading.
    pub fn is_loading(&self) -> bool {
        matches!(self.0, State::Loading { .. })
    }

    pub fn is_valid_purpose(&self, purpose: &str) -> bool {
        match &self.0 {
            // All purposes are considered valid when the registry is still loading.
            //
            // This allows us to have a VRL program compile (and return its type definition), even
            // if it references a purpose that won't eventually be valid.
            //
            // The program is re-compiled when the `remap` transform is built, which is when the
            // registry has loaded, and the next arm matches.
            State::Loading { .. } => true,
            State::Ok { purposes, .. } => purposes.iter().any(|v| v == purpose),
        }
    }

    /// Register the schema of a sink.
    ///
    /// # Errors
    ///
    /// Trying to register a sink when the registry finished loading will result in an error.
    pub fn register_sink(&mut self, key: ComponentKey, schema: schema::Input) -> Result<(), Error> {
        match &mut self.0 {
            State::Loading { sinks, .. } => sinks.insert(key, schema),
            State::Ok { .. } => return Err(Error::InvalidState),
        };

        Ok(())
    }

    /// Register the schema of a source.
    ///
    /// # Errors
    ///
    /// Trying to register a source when the registry finished loading will result in an error.
    pub fn register_source(
        &mut self,
        key: ComponentKey,
        schema: schema::Output,
    ) -> Result<(), Error> {
        match &mut self.0 {
            State::Loading { sources, .. } => sources.insert(key, schema),
            State::Ok { .. } => return Err(Error::InvalidState),
        };

        Ok(())
    }

    /// Register the schema of a transform.
    ///
    /// # Errors
    ///
    /// Trying to register a transform when the registry finished loading will result in an error.
    pub fn register_transform(
        &mut self,
        key: ComponentKey,
        schema: Option<schema::Output>,
        input: schema::Input,
    ) -> Result<(), Error> {
        match &mut self.0 {
            State::Loading { transforms, .. } => transforms.insert(
                key,
                Transform {
                    output: schema,
                    input,
                },
            ),
            State::Ok { .. } => return Err(Error::InvalidState),
        };

        Ok(())
    }
}

/// Internal state for the registry.
#[derive(Debug, Clone)]
enum State {
    /// While loading, the registry does not expose all information about Vector's schema yet.
    Loading {
        sinks: IndexMap<ComponentKey, schema::Input>,
        transforms: IndexMap<ComponentKey, Transform>,
        sources: IndexMap<ComponentKey, schema::Output>,
    },

    /// In the `Ok` state, schema loading is finished and the registry can be safely used.
    Ok { purposes: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct Transform {
    /// The (optional) custom output schema defined by this transform.
    pub output: Option<schema::Output>,

    /// The joined set of schema constraints given to the transform by one or more sinks this
    /// transform feeds into.
    ///
    /// This information is used to determine the valid purpose names exposed by the sinks.
    pub input: schema::Input,
}

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("registry is in an invalid state"))]
    InvalidState,
}

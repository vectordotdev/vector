use enum_dispatch::enum_dispatch;
use serde::Serialize;
use vector_lib::config::GlobalOptions;
use vector_lib::configurable::{configurable_component, Configurable, NamedComponent, ToValue};
use vector_lib::id::{ComponentKey, Inputs};

use crate::enrichment_tables::EnrichmentTables;

use super::dot_graph::GraphConfig;
use super::{SinkConfig, SinkOuter, SourceConfig, SourceOuter};

/// Fully resolved enrichment table component.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct EnrichmentTableOuter<T>
where
    T: Configurable + Serialize + 'static + ToValue + Clone,
{
    #[serde(flatten)]
    pub inner: EnrichmentTables,
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub graph: GraphConfig,
    #[configurable(derived)]
    #[serde(
        default = "Inputs::<T>::default",
        skip_serializing_if = "Inputs::is_empty"
    )]
    pub inputs: Inputs<T>,
}

impl<T> EnrichmentTableOuter<T>
where
    T: Configurable + Serialize + 'static + ToValue + Clone,
{
    pub fn new<I, IET>(inputs: I, inner: IET) -> Self
    where
        I: IntoIterator<Item = T>,
        IET: Into<EnrichmentTables>,
    {
        Self {
            inner: inner.into(),
            graph: Default::default(),
            inputs: Inputs::from_iter(inputs),
        }
    }

    // Components are currently built in a way that they match exactly one of the roles (source,
    // transform, sink, enrichment table). Due to specific requirements of the "memory" enrichment
    // table, it has to fulfill 2 of these roles (sink and enrichment table). To reduce the impact
    // of this very specific requirement, any enrichment table can now be optionally mapped into a
    // sink, but this will only work for a "memory" enrichment table, since other tables will not
    // have a "sink_config" present.
    // This is also not ideal, since `SinkOuter` is not meant to represent the actual configuration,
    // but it should just be a representation of that config used for deserialization.
    // In the future, if more such components come up, it would be good to limit such "Outer"
    // components to deserialization and build up the components and the topology in a more granular
    // way, with each having "modules" for inputs (making them valid as sinks), for healthchecks,
    // for providing outputs, etc.
    pub fn as_sink(&self, default_key: &ComponentKey) -> Option<(ComponentKey, SinkOuter<T>)> {
        self.inner.sink_config(default_key).map(|(key, sink)| {
            (
                key,
                SinkOuter {
                    graph: self.graph.clone(),
                    inputs: self.inputs.clone(),
                    healthcheck_uri: None,
                    healthcheck: Default::default(),
                    buffer: Default::default(),
                    proxy: Default::default(),
                    inner: sink,
                },
            )
        })
    }

    pub fn as_source(&self, default_key: &ComponentKey) -> Option<(ComponentKey, SourceOuter)> {
        self.inner.source_config(default_key).map(|(key, source)| {
            (
                key,
                SourceOuter {
                    graph: self.graph.clone(),
                    sink_acknowledgements: false,
                    proxy: Default::default(),
                    inner: source,
                },
            )
        })
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> EnrichmentTableOuter<U>
    where
        U: Configurable + Serialize + 'static + ToValue + Clone,
    {
        let inputs = self.inputs.iter().map(f).collect::<Vec<_>>();
        self.with_inputs(inputs)
    }

    pub(crate) fn with_inputs<I, U>(self, inputs: I) -> EnrichmentTableOuter<U>
    where
        I: IntoIterator<Item = U>,
        U: Configurable + Serialize + 'static + ToValue + Clone,
    {
        EnrichmentTableOuter {
            inputs: Inputs::from_iter(inputs),
            inner: self.inner,
            graph: self.graph,
        }
    }
}

/// Generalized interface for describing and building enrichment table components.
#[enum_dispatch]
pub trait EnrichmentTableConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    /// Builds the enrichment table with the given globals.
    ///
    /// If the enrichment table is built successfully, `Ok(...)` is returned containing the
    /// enrichment table.
    ///
    /// # Errors
    ///
    /// If an error occurs while building the enrichment table, an error variant explaining the
    /// issue is returned.
    async fn build(
        &self,
        globals: &GlobalOptions,
    ) -> crate::Result<Box<dyn vector_lib::enrichment::Table + Send + Sync>>;

    fn sink_config(
        &self,
        _default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SinkConfig>)> {
        None
    }

    fn source_config(
        &self,
        _default_key: &ComponentKey,
    ) -> Option<(ComponentKey, Box<dyn SourceConfig>)> {
        None
    }
}

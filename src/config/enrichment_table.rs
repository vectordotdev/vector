use enum_dispatch::enum_dispatch;
use serde::Serialize;
use vector_lib::config::GlobalOptions;
use vector_lib::configurable::{configurable_component, Configurable, NamedComponent, ToValue};
use vector_lib::id::Inputs;

use crate::enrichment_tables::EnrichmentTables;

use super::dot_graph::GraphConfig;
use super::{SinkConfig, SinkOuter};

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

    pub fn as_sink(&self) -> Option<SinkOuter<T>> {
        self.inner.sink_config().map(|sink| SinkOuter {
            graph: self.graph.clone(),
            inputs: self.inputs.clone(),
            healthcheck_uri: None,
            healthcheck: Default::default(),
            buffer: Default::default(),
            proxy: Default::default(),
            inner: sink,
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

    fn sink_config(&self) -> Option<Box<dyn SinkConfig>> {
        None
    }
}

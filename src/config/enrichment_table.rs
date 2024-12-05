use enum_dispatch::enum_dispatch;
use vector_lib::config::GlobalOptions;
use vector_lib::configurable::{configurable_component, NamedComponent};

use crate::enrichment_tables::EnrichmentTables;

/// Fully resolved enrichment table component.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct EnrichmentTableOuter {
    #[serde(flatten)]
    pub inner: EnrichmentTables,
}

impl EnrichmentTableOuter {
    pub fn new<I: Into<EnrichmentTables>>(inner: I) -> Self {
        Self {
            inner: inner.into(),
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
}

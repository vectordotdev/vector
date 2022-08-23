use std::collections::HashMap;

use async_trait::async_trait;
use vector_config::{configurable_component, NamedComponent};
use vector_core::config::{AcknowledgementsConfig, GlobalOptions, LogNamespace, Output};

use super::{schema, ComponentKey, ProxyConfig, Resource};
use crate::{
    shutdown::ShutdownSignal,
    sources::{self, Sources},
    SourceSender,
};

/// Fully resolved source component.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct SourceOuter {
    #[configurable(derived)]
    #[serde(
        default,
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub proxy: ProxyConfig,

    #[serde(default, skip)]
    pub sink_acknowledgements: bool,

    #[serde(flatten)]
    pub(crate) inner: Sources,
}

impl SourceOuter {
    pub(crate) fn new(source: Sources) -> Self {
        Self {
            proxy: Default::default(),
            sink_acknowledgements: false,
            inner: source,
        }
    }
}

#[async_trait]
pub trait SourceConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source>;

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output>;

    /// Resources that the source is using.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    fn can_acknowledge(&self) -> bool;
}

pub struct SourceContext {
    pub key: ComponentKey,
    pub globals: GlobalOptions,
    pub shutdown: ShutdownSignal,
    pub out: SourceSender,
    pub proxy: ProxyConfig,
    pub acknowledgements: bool,
    pub schema: schema::Options,

    /// Tracks the schema IDs assigned to schemas exposed by the source.
    ///
    /// Given a source can expose multiple [`Output`] channels, the ID is tied to the identifier of
    /// that `Output`.
    pub schema_definitions: HashMap<Option<String>, schema::Definition>,
}

impl SourceContext {
    #[cfg(test)]
    pub fn new_shutdown(
        key: &ComponentKey,
        out: SourceSender,
    ) -> (Self, crate::shutdown::SourceShutdownCoordinator) {
        let mut shutdown = crate::shutdown::SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(key);
        (
            Self {
                key: key.clone(),
                globals: GlobalOptions::default(),
                shutdown: shutdown_signal,
                out,
                proxy: Default::default(),
                acknowledgements: false,
                schema_definitions: HashMap::default(),
                schema: Default::default(),
            },
            shutdown,
        )
    }

    #[cfg(test)]
    pub fn new_test(
        out: SourceSender,
        schema_definitions: Option<HashMap<Option<String>, schema::Definition>>,
    ) -> Self {
        Self {
            key: ComponentKey::from("default"),
            globals: GlobalOptions::default(),
            shutdown: ShutdownSignal::noop(),
            out,
            proxy: Default::default(),
            acknowledgements: false,
            schema_definitions: schema_definitions.unwrap_or_default(),
            schema: Default::default(),
        }
    }

    pub fn do_acknowledgements(&self, config: &AcknowledgementsConfig) -> bool {
        if config.enabled() {
            warn!(
                message = "Enabling `acknowledgements` on sources themselves is deprecated in favor of enabling them in the sink configuration, and will be removed in a future version.",
                component_name = self.key.id(),
            );
        }

        config
            .merge_default(&self.globals.acknowledgements)
            .merge_default(&self.acknowledgements.into())
            .enabled()
    }

    /// Gets the log namespacing to use. The passed in value is from the source itself
    /// and will override any global default if it's set.
    pub fn log_namespace(&self, namespace: Option<bool>) -> LogNamespace {
        namespace
            .or(self.schema.log_namespace)
            .unwrap_or(false)
            .into()
    }
}

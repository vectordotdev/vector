use std::collections::HashMap;

use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use vector_config::{configurable_component, NamedComponent};
use vector_core::{
    config::{AcknowledgementsConfig, GlobalOptions, LogNamespace, Output},
    source::Source,
};

use super::{schema, ComponentKey, ProxyConfig, Resource};
use crate::{shutdown::ShutdownSignal, sources::Sources, SourceSender};

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
    pub(crate) fn new<I: Into<Sources>>(inner: I) -> Self {
        Self {
            proxy: Default::default(),
            sink_acknowledgements: false,
            inner: inner.into(),
        }
    }
}

/// Generalized interface for describing and building source components.
#[async_trait]
#[enum_dispatch]
pub trait SourceConfig: NamedComponent + core::fmt::Debug + Send + Sync {
    /// Builds the source with the given context.
    ///
    /// If the source is built successfully, `Ok(...)` is returned containing the source.
    ///
    /// # Errors
    ///
    /// If an error occurs while building the source, an error variant explaining the issue is
    /// returned.
    async fn build(&self, cx: SourceContext) -> crate::Result<Source>;

    /// Gets the list of outputs exposed by this source.
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output>;

    /// Gets the list of resources, if any, used by this source.
    ///
    /// Resources represent dependencies -- network ports, file descriptors, and so on -- that
    /// cannot be shared between components at runtime. This ensures that components can not be
    /// configured in a way that would deadlock the spawning of a topology, and as well, allows
    /// Vector to determine the correct order for rebuilding a topology during configuration reload
    /// when resources must first be reclaimed before being reassigned, and so on.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    /// Whether or not this source can acknowledge the events it emits.
    ///
    /// Generally, Vector uses acknowledgements to track when an event has finally been processed,
    /// either successfully or unsuccessfully. While it is used internally in some areas, such as
    /// within disk buffers for knowing when a message can be deleted from the buffer, it is
    /// primarily used to signal back to a source that a message has been successfully (durably)
    /// processed or not.
    ///
    /// By exposing whether or not a source supports acknowledgements, we can avoid situations where
    /// using acknowledgements would only add processing overhead for no benefit to the source, as
    /// well as emit contextual warnings when end-to-end acknowledgements are enabled, but the
    /// topology as configured does not actually support the use of end-to-end acknowledgements.
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

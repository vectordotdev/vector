use std::cell::RefCell;
use std::collections::HashMap;

use async_trait::async_trait;
use dyn_clone::DynClone;
use vector_lib::configurable::attributes::CustomAttribute;
use vector_lib::configurable::schema::{SchemaGenerator, SchemaObject};
use vector_lib::configurable::{
    configurable_component, Configurable, GenerateError, Metadata, NamedComponent,
};
use vector_lib::{
    config::{
        AcknowledgementsConfig, GlobalOptions, LogNamespace, SourceAcknowledgementsConfig,
        SourceOutput,
    },
    source::Source,
};

use super::{schema, ComponentKey, ProxyConfig, Resource};
use crate::{extra_context::ExtraContext, shutdown::ShutdownSignal, SourceSender};

pub type BoxedSource = Box<dyn SourceConfig>;

impl Configurable for BoxedSource {
    fn referenceable_name() -> Option<&'static str> {
        Some("vector::sources::Sources")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("Configurable sources in Vector.");
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "internal"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tag_field", "type"));
        metadata
    }

    fn generate_schema(gen: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
        vector_lib::configurable::component::SourceDescription::generate_schemas(gen)
    }
}

impl<T: SourceConfig + 'static> From<T> for BoxedSource {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

/// Fully resolved source component.
#[configurable_component]
#[configurable(metadata(docs::component_base_type = "source"))]
#[derive(Clone, Debug)]
pub struct SourceOuter {
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub proxy: ProxyConfig,

    #[serde(default, skip)]
    pub sink_acknowledgements: bool,

    #[configurable(metadata(docs::hidden))]
    #[serde(flatten)]
    pub(crate) inner: BoxedSource,
}

impl SourceOuter {
    pub(crate) fn new<I: Into<BoxedSource>>(inner: I) -> Self {
        Self {
            proxy: Default::default(),
            sink_acknowledgements: false,
            inner: inner.into(),
        }
    }
}

/// Generalized interface for describing and building source components.
#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SourceConfig: DynClone + NamedComponent + core::fmt::Debug + Send + Sync {
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
    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput>;

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

dyn_clone::clone_trait_object!(SourceConfig);

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
    /// Given a source can expose multiple [`SourceOutput`] channels, the ID is tied to the identifier of
    /// that `SourceOutput`.
    pub schema_definitions: HashMap<Option<String>, schema::Definition>,

    /// Extra context data provided by the running app and shared across all components. This can be
    /// used to pass shared settings or other data from outside the components.
    pub extra_context: ExtraContext,
}

impl SourceContext {
    #[cfg(test)]
    pub fn new_shutdown(
        key: &ComponentKey,
        out: SourceSender,
    ) -> (Self, crate::shutdown::SourceShutdownCoordinator) {
        let mut shutdown = crate::shutdown::SourceShutdownCoordinator::default();
        let (shutdown_signal, _) = shutdown.register_source(key, false);
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
                extra_context: Default::default(),
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
            extra_context: Default::default(),
        }
    }

    pub fn do_acknowledgements(&self, config: SourceAcknowledgementsConfig) -> bool {
        let config = AcknowledgementsConfig::from(config);
        if config.enabled() {
            warn!(
                message = "Enabling `acknowledgements` on sources themselves is deprecated in favor of enabling them in the sink configuration, and will be removed in a future version.",
                component_id = self.key.id(),
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

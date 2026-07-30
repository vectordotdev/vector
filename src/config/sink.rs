use std::{cell::RefCell, path::PathBuf, time::Duration};

use async_trait::async_trait;
use dyn_clone::DynClone;
use serde::Serialize;
use serde_with::serde_as;
use vector_lib::{
    buffers::{BufferConfig, BufferType},
    config::{AcknowledgementsConfig, GlobalOptions, Input},
    configurable::{
        Configurable, GenerateError, Metadata, NamedComponent,
        attributes::CustomAttribute,
        configurable_component,
        schema::{SchemaGenerator, SchemaObject},
    },
    id::Inputs,
    sink::VectorSink,
};
use vector_vrl_metrics::MetricsStorage;

use super::{ComponentKey, ProxyConfig, Resource, dot_graph::GraphConfig, schema};
use crate::{
    extra_context::ExtraContext,
    sinks::{Healthcheck, util::UriSerde},
};

pub type BoxedSink = Box<dyn SinkConfig>;

impl Configurable for BoxedSink {
    fn referenceable_name() -> Option<&'static str> {
        Some("vector::sinks::Sinks")
    }

    fn metadata() -> Metadata {
        let mut metadata = Metadata::default();
        metadata.set_description("Configurable sinks in Vector.");
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "internal"));
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tag_field", "type"));
        metadata
    }

    fn generate_schema(
        generator: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        vector_lib::configurable::component::SinkDescription::generate_schemas(generator)
    }
}

impl<T: SinkConfig + 'static> From<T> for BoxedSink {
    fn from(value: T) -> Self {
        Box::new(value)
    }
}

/// Fully resolved sink component.
#[configurable_component]
#[configurable(metadata(docs::component_base_type = "sink"))]
#[derive(Clone, Debug)]
pub struct SinkOuter<T>
where
    T: Configurable + Serialize + 'static,
{
    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub graph: GraphConfig,

    #[configurable(derived)]
    pub inputs: Inputs<T>,

    /// The full URI to make HTTP healthcheck requests to.
    ///
    /// This must be a valid URI, which requires at least the scheme and host. All other
    /// components -- port, path, etc -- are allowed as well.
    #[configurable(deprecated, metadata(docs::hidden), validation(format = "uri"))]
    pub healthcheck_uri: Option<UriSerde>,

    #[configurable(derived, metadata(docs::advanced))]
    #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
    pub healthcheck: SinkHealthcheckOptions,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub buffer: BufferConfig,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    pub proxy: ProxyConfig,

    #[serde(flatten)]
    #[configurable(metadata(docs::hidden))]
    pub inner: BoxedSink,
}

impl<T> SinkOuter<T>
where
    T: Configurable + Serialize,
{
    pub fn new<I, IS>(inputs: I, inner: IS) -> SinkOuter<T>
    where
        I: IntoIterator<Item = T>,
        IS: Into<BoxedSink>,
    {
        SinkOuter {
            inputs: Inputs::from_iter(inputs),
            buffer: Default::default(),
            healthcheck: SinkHealthcheckOptions::default(),
            healthcheck_uri: None,
            inner: inner.into(),
            proxy: Default::default(),
            graph: Default::default(),
        }
    }

    pub fn resources(&self, id: &ComponentKey) -> Vec<Resource> {
        let mut resources = self.inner.resources();
        for stage in self.buffer.stages() {
            match stage {
                BufferType::Memory { .. } => {}
                BufferType::DiskV2 { .. } => resources.push(Resource::DiskBuffer(id.to_string())),
            }
        }
        resources
    }

    pub fn healthcheck(&self) -> SinkHealthcheckOptions {
        if self.healthcheck_uri.is_some() && self.healthcheck.uri.is_some() {
            warn!(
                "Both `healthcheck.uri` and `healthcheck_uri` options are specified. Using value of `healthcheck.uri`."
            )
        } else if self.healthcheck_uri.is_some() {
            warn!(
                "The `healthcheck_uri` option has been deprecated, use `healthcheck.uri` instead."
            )
        }
        SinkHealthcheckOptions {
            uri: self
                .healthcheck
                .uri
                .clone()
                .or_else(|| self.healthcheck_uri.clone()),
            ..self.healthcheck.clone()
        }
    }

    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> SinkOuter<U>
    where
        U: Configurable + Serialize,
    {
        let inputs = self.inputs.iter().map(f).collect::<Vec<_>>();
        self.with_inputs(inputs)
    }

    pub(crate) fn with_inputs<I, U>(self, inputs: I) -> SinkOuter<U>
    where
        I: IntoIterator<Item = U>,
        U: Configurable + Serialize,
    {
        SinkOuter {
            inputs: Inputs::from_iter(inputs),
            inner: self.inner,
            buffer: self.buffer,
            healthcheck: self.healthcheck,
            healthcheck_uri: self.healthcheck_uri,
            proxy: self.proxy,
            graph: self.graph,
        }
    }
}

/// Healthcheck configuration.
#[serde_as]
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
pub struct SinkHealthcheckOptions {
    /// Whether or not to check the health of the sink when Vector starts up.
    pub enabled: bool,

    /// Timeout duration for healthcheck in seconds.
    #[serde_as(as = "serde_with::DurationSecondsWithFrac<f64>")]
    #[serde(
        default = "default_healthcheck_timeout",
        skip_serializing_if = "is_default_healthcheck_timeout"
    )]
    pub timeout: Duration,

    /// The full URI to make HTTP healthcheck requests to.
    ///
    /// This must be a valid URI, which requires at least the scheme and host. All other
    /// components -- port, path, etc -- are allowed as well.
    #[configurable(validation(format = "uri"))]
    pub uri: Option<UriSerde>,
}

const fn default_healthcheck_timeout() -> Duration {
    Duration::from_secs(10)
}

fn is_default_healthcheck_timeout(timeout: &Duration) -> bool {
    timeout == &default_healthcheck_timeout()
}

impl Default for SinkHealthcheckOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            uri: None,
            timeout: default_healthcheck_timeout(),
        }
    }
}

impl From<bool> for SinkHealthcheckOptions {
    fn from(enabled: bool) -> Self {
        Self {
            enabled,
            ..Default::default()
        }
    }
}

impl From<UriSerde> for SinkHealthcheckOptions {
    fn from(uri: UriSerde) -> Self {
        Self {
            uri: Some(uri),
            ..Default::default()
        }
    }
}

/// Generalized interface for describing and building sink components.
#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SinkConfig: DynClone + NamedComponent + core::fmt::Debug + Send + Sync {
    /// Phase 1: validates structural constraints that do not require environment resources --
    /// invalid URIs, out-of-range values, duplicate keys, template confinement -- and returns the
    /// values derived along the way for the later phases to consume.
    ///
    /// Most of these checks are really *fallible pure constructors*: [`Template::confine`] yields a
    /// [`ConfinedTemplate`], `choose_one` yields an `Auth`, `into_batcher_settings` yields
    /// `BatcherSettings`. Returning them in a [`StructureValidated`] state is what keeps `build`
    /// from recomputing every check and drifting from this one.
    ///
    /// Called during config compilation, so errors are reported on both `vector validate` and
    /// normal startup/reload.
    ///
    /// # Contract
    ///
    /// Implementations must be **pure**: no network, no filesystem, no credential resolution, no
    /// task spawning. Prefer accumulating every error into the returned `Vec` rather than returning
    /// only the first, so a single `vector validate` run reports all of a sink's problems at once.
    ///
    /// # Errors
    ///
    /// If validation does not succeed, an error variant containing a list of all validation errors
    /// is returned.
    ///
    /// [`Template::confine`]: crate::template::Template::confine
    /// [`ConfinedTemplate`]: crate::template::ConfinedTemplate
    fn validate_structure(&self) -> Result<Box<dyn StructureValidated>, Vec<String>> {
        Ok(Box::new(UnphasedSink))
    }

    /// Builds the sink with the given context.
    ///
    /// The default implementation drives the three phases in order, which is the path a migrated
    /// sink takes: it implements [`SinkConfig::validate_structure`] plus the two phase traits and
    /// does not override this method. Sinks not yet migrated override this directly.
    ///
    /// # Errors
    ///
    /// If an error occurs while building the sink, an error variant explaining the issue is
    /// returned.
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let validation_cx = ValidationContext::from_sink_context(&cx);
        let structure = self
            .validate_structure()
            .map_err(|errors| errors.join("; "))?;
        let state = structure
            .validate_state(&validation_cx)
            .map_err(|errors| errors.join("; "))?;
        state.build(cx).await
    }

    /// Gets the input configuration for this sink.
    fn input(&self) -> Input;

    /// Gets the files to watch to trigger reload
    fn files_to_watch(&self) -> Vec<&PathBuf> {
        Vec::new()
    }

    /// Gets the list of resources, if any, used by this sink.
    ///
    /// Resources represent dependencies -- network ports, file descriptors, and so on -- that
    /// cannot be shared between components at runtime. This ensures that components can not be
    /// configured in a way that would deadlock the spawning of a topology, and as well, allows
    /// Vector to determine the correct order for rebuilding a topology during configuration reload
    /// when resources must first be reclaimed before being reassigned, and so on.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    /// Returns this sink's template-confinement config, if it supports confinement.
    ///
    /// The topology uses this to own the `vector_security_confinement_disabled`
    /// gauge for the sink's lifetime. `None` (the default) means the sink does
    /// not participate in template confinement and emits no gauge.
    fn confinement_config(&self) -> Option<&crate::template::ConfinementConfig> {
        None
    }

    /// Gets the acknowledgements configuration for this sink.
    fn acknowledgements(&self) -> &AcknowledgementsConfig;
}

/// The environment available to phase 2, [`StructureValidated::validate_state`].
///
/// This deliberately carries only what the *running app* knows and the config cannot: settings
/// merged from the global config and the process environment. Anything the user wrote in the sink's
/// own config (TLS options, endpoints, credentials) is already owned by the phase-1 state.
#[derive(Clone, Debug, Default)]
pub struct ValidationContext {
    /// Proxy settings, already merged with the global config and environment.
    pub proxy: ProxyConfig,
}

impl ValidationContext {
    /// Extracts the phase-2 environment from a full [`SinkContext`].
    pub fn from_sink_context(cx: &SinkContext) -> Self {
        Self {
            proxy: cx.proxy.clone(),
        }
    }
}

/// Phase 1 output: a config proven well-formed, carrying the values the later phases need.
///
/// Produced by [`SinkConfig::validate_structure`]. Because the only way to reach phase 2 is to hold
/// one of these, and the only way to reach [`StateValidated::build`] is to hold a phase-2 state,
/// the compiler enforces the ordering that would otherwise be a documented convention.
///
/// # Contract
///
/// Phase 2 may resolve settings and open *inert* clients. It must not reach the network and must
/// not spawn background tasks; anything that talks to a remote endpoint belongs in
/// [`StateValidated::build`].
pub trait StructureValidated: Send {
    /// Advances to phase 2, resolving anything that needs the running app's environment.
    ///
    /// # Errors
    ///
    /// Returns every error found, not just the first.
    fn validate_state(
        self: Box<Self>,
        cx: &ValidationContext,
    ) -> Result<Box<dyn StateValidated>, Vec<String>>;
}

/// Phase 2 output: a state from which the sink can be constructed.
#[async_trait]
pub trait StateValidated: Send {
    /// Final phase: constructs the sink and its healthcheck. Side effects are allowed here,
    /// including reaching the endpoint. Must not spawn background tasks; those belong in the
    /// sink's own `run()` future so a rolled-back reload leaves nothing running.
    async fn build(self: Box<Self>, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)>;
}

/// Phase-1 state for a sink that has not been migrated to phased validation.
///
/// Such sinks override [`SinkConfig::build`] directly, so the default `build` that drives the
/// phases is never reached and `validate_state` is never called. The error exists only so a sink
/// that implements *neither* fails loudly instead of silently doing nothing.
struct UnphasedSink;

impl StructureValidated for UnphasedSink {
    fn validate_state(
        self: Box<Self>,
        _cx: &ValidationContext,
    ) -> Result<Box<dyn StateValidated>, Vec<String>> {
        Err(vec![
            "sink implements neither phased validation nor `build`".to_string(),
        ])
    }
}

dyn_clone::clone_trait_object!(SinkConfig);

#[derive(Clone, Debug)]
pub struct SinkContext {
    pub healthcheck: SinkHealthcheckOptions,
    pub globals: GlobalOptions,
    pub enrichment_tables: vector_lib::enrichment::TableRegistry,
    pub metrics_storage: MetricsStorage,
    pub proxy: ProxyConfig,
    pub schema: schema::Options,
    pub app_name: String,
    pub app_name_slug: String,

    /// Extra context data provided by the running app and shared across all components. This can be
    /// used to pass shared settings or other data from outside the components.
    pub extra_context: ExtraContext,
}

impl Default for SinkContext {
    fn default() -> Self {
        Self {
            healthcheck: Default::default(),
            globals: Default::default(),
            enrichment_tables: Default::default(),
            metrics_storage: Default::default(),
            proxy: Default::default(),
            schema: Default::default(),
            app_name: crate::get_app_name().to_string(),
            app_name_slug: crate::get_slugified_app_name(),
            extra_context: Default::default(),
        }
    }
}

impl SinkContext {
    pub const fn globals(&self) -> &GlobalOptions {
        &self.globals
    }

    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }
}

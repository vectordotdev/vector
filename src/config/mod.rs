use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display, Formatter},
    hash::Hash,
    net::SocketAddr,
    path::PathBuf,
};

use async_trait::async_trait;
use component::ComponentDescription;
use indexmap::IndexMap; // IndexMap preserves insertion order, allowing us to output errors in the same order they are present in the file
use serde::{Deserialize, Serialize};
use vector_buffers::{Acker, BufferConfig, BufferType};
pub use vector_core::{
    config::{AcknowledgementsConfig, DataType, GlobalOptions, Output},
    transform::{ExpandType, TransformConfig, TransformContext},
};

use crate::{
    conditions,
    event::Metric,
    shutdown::ShutdownSignal,
    sinks::{self, util::UriSerde},
    sources,
    transforms::noop::Noop,
    SourceSender,
};

pub mod api;
mod builder;
mod compiler;
pub mod component;
#[cfg(feature = "datadog-pipelines")]
pub mod datadog;
mod diff;
pub mod format;
mod graph;
mod id;
mod loading;
pub mod provider;
mod recursive;
mod unit_test;
mod validation;
mod vars;
pub mod watcher;

pub use builder::ConfigBuilder;
pub use diff::ConfigDiff;
pub use format::{Format, FormatHint};
pub use id::{ComponentKey, OutputId};
pub use loading::{
    load, load_builder_from_paths, load_from_paths, load_from_paths_with_provider, load_from_str,
    merge_path_lists, process_paths, CONFIG_PATHS,
};
pub use unit_test::{build_unit_tests, build_unit_tests_main, UnitTestResult};
pub use validation::warnings;
pub use vector_core::config::{log_schema, proxy::ProxyConfig, LogSchema};

/// Loads Log Schema from configurations and sets global schema.
/// Once this is done, configurations can be correctly loaded using
/// configured log schema defaults.
/// If deny is set, will panic if schema has already been set.
pub fn init_log_schema(config_paths: &[ConfigPath], deny_if_set: bool) -> Result<(), Vec<String>> {
    vector_core::config::init_log_schema(
        || {
            let (builder, _) = load_builder_from_paths(config_paths)?;
            Ok(builder.global.log_schema)
        },
        deny_if_set,
    )
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum ConfigPath {
    File(PathBuf, FormatHint),
    Dir(PathBuf),
}

impl<'a> From<&'a ConfigPath> for &'a PathBuf {
    fn from(config_path: &'a ConfigPath) -> &'a PathBuf {
        match config_path {
            ConfigPath::File(path, _) => path,
            ConfigPath::Dir(path) => path,
        }
    }
}

impl ConfigPath {
    pub const fn as_dir(&self) -> Option<&PathBuf> {
        match self {
            Self::Dir(path) => Some(path),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Config {
    #[cfg(feature = "api")]
    pub api: api::Options,
    pub version: Option<String>,
    #[cfg(feature = "datadog-pipelines")]
    pub datadog: Option<datadog::Options>,
    pub global: GlobalOptions,
    pub healthchecks: HealthcheckOptions,
    pub sources: IndexMap<ComponentKey, SourceOuter>,
    pub sinks: IndexMap<ComponentKey, SinkOuter<OutputId>>,
    pub transforms: IndexMap<ComponentKey, TransformOuter<OutputId>>,
    pub enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter>,
    tests: Vec<TestDefinition>,
    expansions: IndexMap<ComponentKey, Vec<ComponentKey>>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(default)]
pub struct HealthcheckOptions {
    pub enabled: bool,
    pub require_healthy: bool,
}

impl HealthcheckOptions {
    pub fn set_require_healthy(&mut self, require_healthy: impl Into<Option<bool>>) {
        if let Some(require_healthy) = require_healthy.into() {
            self.require_healthy = require_healthy;
        }
    }

    fn merge(&mut self, other: Self) {
        self.enabled &= other.enabled;
        self.require_healthy |= other.require_healthy;
    }
}

impl Default for HealthcheckOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            require_healthy: false,
        }
    }
}

pub trait GenerateConfig {
    fn generate_config() -> toml::Value;
}

#[macro_export]
macro_rules! impl_generate_config_from_default {
    ($type:ty) => {
        impl $crate::config::GenerateConfig for $type {
            fn generate_config() -> toml::Value {
                toml::Value::try_from(&Self::default()).unwrap()
            }
        }
    };
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SourceOuter {
    #[serde(
        default,
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub proxy: ProxyConfig,
    #[serde(flatten)]
    pub(super) inner: Box<dyn SourceConfig>,
}

impl SourceOuter {
    pub(crate) fn new(source: impl SourceConfig + 'static) -> Self {
        Self {
            inner: Box::new(source),
            proxy: Default::default(),
        }
    }
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SourceConfig: core::fmt::Debug + Send + Sync {
    async fn build(&self, cx: SourceContext) -> crate::Result<sources::Source>;

    fn outputs(&self) -> Vec<Output>;

    fn source_type(&self) -> &'static str;

    /// Resources that the source is using.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}

pub struct SourceContext {
    pub key: ComponentKey,
    pub globals: GlobalOptions,
    pub shutdown: ShutdownSignal,
    pub out: SourceSender,
    pub proxy: ProxyConfig,
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
            },
            shutdown,
        )
    }

    #[cfg(test)]
    pub fn new_test(out: SourceSender) -> Self {
        Self {
            key: ComponentKey::from("default"),
            globals: GlobalOptions::default(),
            shutdown: ShutdownSignal::noop(),
            out,
            proxy: Default::default(),
        }
    }
}

pub type SourceDescription = ComponentDescription<Box<dyn SourceConfig>>;

inventory::collect!(SourceDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct SinkOuter<T> {
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,
    // We are accepting this option for backward compatibility.
    healthcheck_uri: Option<UriSerde>,

    // We are accepting bool for backward compatibility.
    #[serde(deserialize_with = "crate::serde::bool_or_struct")]
    #[serde(default)]
    healthcheck: SinkHealthcheckOptions,

    #[serde(default)]
    pub buffer: BufferConfig,

    #[serde(
        default,
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    proxy: ProxyConfig,

    #[serde(flatten)]
    pub inner: Box<dyn SinkConfig>,
}

impl<T> SinkOuter<T> {
    pub fn new(inputs: Vec<T>, inner: Box<dyn SinkConfig>) -> SinkOuter<T> {
        SinkOuter {
            inputs,
            buffer: Default::default(),
            healthcheck: SinkHealthcheckOptions::default(),
            healthcheck_uri: None,
            inner,
            proxy: Default::default(),
        }
    }

    pub fn resources(&self, id: &ComponentKey) -> Vec<Resource> {
        let mut resources = self.inner.resources();
        for stage in self.buffer.stages() {
            match stage {
                BufferType::MemoryV1 { .. } | BufferType::MemoryV2 { .. } => {}
                BufferType::DiskV1 { .. } | BufferType::DiskV2 { .. } => {
                    resources.push(Resource::DiskBuffer(id.to_string()))
                }
            }
        }
        resources
    }

    pub fn healthcheck(&self) -> SinkHealthcheckOptions {
        if self.healthcheck_uri.is_some() && self.healthcheck.uri.is_some() {
            warn!("Both `healthcheck.uri` and `healthcheck_uri` options are specified. Using value of `healthcheck.uri`.")
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

    fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> SinkOuter<U> {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    fn with_inputs<U>(self, inputs: Vec<U>) -> SinkOuter<U> {
        SinkOuter {
            inputs,
            inner: self.inner,
            buffer: self.buffer,
            healthcheck: self.healthcheck,
            healthcheck_uri: self.healthcheck_uri,
            proxy: self.proxy,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default)]
pub struct SinkHealthcheckOptions {
    pub enabled: bool,
    pub uri: Option<UriSerde>,
}

impl Default for SinkHealthcheckOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            uri: None,
        }
    }
}

impl From<bool> for SinkHealthcheckOptions {
    fn from(enabled: bool) -> Self {
        Self { enabled, uri: None }
    }
}

impl From<UriSerde> for SinkHealthcheckOptions {
    fn from(uri: UriSerde) -> Self {
        Self {
            enabled: true,
            uri: Some(uri),
        }
    }
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug + Send + Sync {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(sinks::VectorSink, sinks::Healthcheck)>;

    fn input_type(&self) -> DataType;

    fn sink_type(&self) -> &'static str;

    /// Resources that the sink is using.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct SinkContext {
    pub acker: Acker,
    pub healthcheck: SinkHealthcheckOptions,
    pub globals: GlobalOptions,
    pub proxy: ProxyConfig,
}

impl SinkContext {
    #[cfg(test)]
    pub fn new_test() -> Self {
        Self {
            acker: Acker::passthrough(),
            healthcheck: SinkHealthcheckOptions::default(),
            globals: GlobalOptions::default(),
            proxy: ProxyConfig::default(),
        }
    }

    pub fn acker(&self) -> Acker {
        self.acker.clone()
    }

    pub const fn globals(&self) -> &GlobalOptions {
        &self.globals
    }

    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }
}

pub type SinkDescription = ComponentDescription<Box<dyn SinkConfig>>;

inventory::collect!(SinkDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter<T> {
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,
    #[serde(flatten)]
    pub inner: Box<dyn TransformConfig>,
}

impl<T> TransformOuter<T> {
    fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> TransformOuter<U> {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    fn with_inputs<U>(self, inputs: Vec<U>) -> TransformOuter<U> {
        TransformOuter {
            inputs,
            inner: self.inner,
        }
    }
}

impl TransformOuter<String> {
    pub(crate) fn expand(
        mut self,
        key: ComponentKey,
        parent_types: &HashSet<&'static str>,
        transforms: &mut IndexMap<ComponentKey, TransformOuter<String>>,
        expansions: &mut IndexMap<ComponentKey, Vec<ComponentKey>>,
    ) -> Result<(), String> {
        if !self.inner.nestable(parent_types) {
            return Err(format!(
                "the component {} cannot be nested in {:?}",
                self.inner.transform_type(),
                parent_types
            ));
        }

        let expansion = self
            .inner
            .expand()
            .map_err(|err| format!("failed to expand transform '{}': {}", key, err))?;

        let mut ptypes = parent_types.clone();
        ptypes.insert(self.inner.transform_type());

        if let Some((expanded, expand_type)) = expansion {
            let mut children = Vec::new();
            let mut inputs = self.inputs.clone();

            for (name, content) in expanded {
                let full_name = key.join(name);

                let child = TransformOuter {
                    inputs,
                    inner: content,
                };
                child.expand(full_name.clone(), &ptypes, transforms, expansions)?;
                children.push(full_name.clone());

                inputs = match expand_type {
                    ExpandType::Parallel { .. } => self.inputs.clone(),
                    ExpandType::Serial { .. } => vec![full_name.to_string()],
                }
            }

            if matches!(expand_type, ExpandType::Parallel { aggregates: true }) {
                transforms.insert(
                    key.clone(),
                    TransformOuter {
                        inputs: children.iter().map(ToString::to_string).collect(),
                        inner: Box::new(Noop),
                    },
                );
                children.push(key.clone());
            } else if matches!(expand_type, ExpandType::Serial { alias: true }) {
                transforms.insert(
                    key.clone(),
                    TransformOuter {
                        inputs,
                        inner: Box::new(Noop),
                    },
                );
                children.push(key.clone());
            }

            expansions.insert(key.clone(), children);
        } else {
            transforms.insert(key, self);
        }
        Ok(())
    }
}

pub type TransformDescription = ComponentDescription<Box<dyn TransformConfig>>;

inventory::collect!(TransformDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct EnrichmentTableOuter {
    #[serde(flatten)]
    pub inner: Box<dyn EnrichmentTableConfig>,
}

impl EnrichmentTableOuter {
    pub fn new(inner: Box<dyn EnrichmentTableConfig>) -> Self {
        EnrichmentTableOuter { inner }
    }
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait EnrichmentTableConfig: core::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    async fn build(
        &self,
        globals: &GlobalOptions,
    ) -> crate::Result<Box<dyn enrichment::Table + Send + Sync>>;
}

pub type EnrichmentTableDescription = ComponentDescription<Box<dyn EnrichmentTableConfig>>;

inventory::collect!(EnrichmentTableDescription);

/// Unique thing, like port, of which only one owner can be.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Resource {
    Port(SocketAddr, Protocol),
    SystemFdOffset(usize),
    Stdin,
    DiskBuffer(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Copy)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Resource {
    pub const fn tcp(addr: SocketAddr) -> Self {
        Self::Port(addr, Protocol::Tcp)
    }

    pub const fn udp(addr: SocketAddr) -> Self {
        Self::Port(addr, Protocol::Udp)
    }

    /// From given components returns all that have a resource conflict with any other component.
    pub fn conflicts<K: Eq + Hash + Clone>(
        components: impl IntoIterator<Item = (K, Vec<Resource>)>,
    ) -> HashMap<Resource, HashSet<K>> {
        let mut resource_map = HashMap::<Resource, HashSet<K>>::new();
        let mut unspecified = Vec::new();

        // Find equality based conflicts
        for (key, resources) in components {
            for resource in resources {
                if let Resource::Port(address, protocol) = &resource {
                    if address.ip().is_unspecified() {
                        unspecified.push((key.clone(), address.port(), *protocol));
                    }
                }

                resource_map
                    .entry(resource)
                    .or_default()
                    .insert(key.clone());
            }
        }

        // Port with unspecified address will bind to all network interfaces
        // so we have to check for all Port resources if they share the same
        // port.
        for (key, port, protocol0) in unspecified {
            for (resource, components) in resource_map.iter_mut() {
                if let Resource::Port(address, protocol) = resource {
                    if address.port() == port && &protocol0 == protocol {
                        components.insert(key.clone());
                    }
                }
            }
        }

        resource_map.retain(|_, components| components.len() > 1);

        resource_map
    }
}

impl Display for Protocol {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Protocol::Udp => write!(fmt, "udp"),
            Protocol::Tcp => write!(fmt, "tcp"),
        }
    }
}

impl Display for Resource {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Resource::Port(address, protocol) => write!(fmt, "{} {}", protocol, address),
            Resource::SystemFdOffset(offset) => write!(fmt, "systemd {}th socket", offset + 1),
            Resource::Stdin => write!(fmt, "stdin"),
            Resource::DiskBuffer(name) => write!(fmt, "disk buffer {:?}", name),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestDefinition<T = OutputId> {
    pub name: String,
    pub input: Option<TestInput>,
    #[serde(default)]
    pub inputs: Vec<TestInput>,
    #[serde(default)]
    pub outputs: Vec<TestOutput<T>>,
    #[serde(default)]
    pub no_outputs_from: Vec<T>,
}

impl TestDefinition<String> {
    fn resolve_outputs(self, graph: &graph::Graph) -> TestDefinition<OutputId> {
        let TestDefinition {
            name,
            input,
            inputs,
            outputs,
            no_outputs_from,
        } = self;

        let output_map = graph.input_map().expect("ambiguous outputs");

        let outputs = outputs
            .into_iter()
            .map(|old| TestOutput {
                extract_from: output_map.get(&old.extract_from).unwrap().clone(),
                conditions: old.conditions,
            })
            .collect();

        let no_outputs_from = no_outputs_from
            .into_iter()
            .map(|o| output_map.get(&o).unwrap().clone())
            .collect();

        TestDefinition {
            name,
            input,
            inputs,
            outputs,
            no_outputs_from,
        }
    }
}

impl TestDefinition<OutputId> {
    fn stringify(self) -> TestDefinition<String> {
        let TestDefinition {
            name,
            input,
            inputs,
            outputs,
            no_outputs_from,
        } = self;

        let outputs = outputs
            .into_iter()
            .map(|old| TestOutput {
                extract_from: old.extract_from.to_string(),
                conditions: old.conditions,
            })
            .collect();

        let no_outputs_from = no_outputs_from.iter().map(ToString::to_string).collect();

        TestDefinition {
            name,
            input,
            inputs,
            outputs,
            no_outputs_from,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum TestInputValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestInput {
    pub insert_at: ComponentKey,
    #[serde(default = "default_test_input_type", rename = "type")]
    pub type_str: String,
    pub value: Option<String>,
    pub log_fields: Option<IndexMap<String, TestInputValue>>,
    pub metric: Option<Metric>,
}

fn default_test_input_type() -> String {
    "raw".to_string()
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestOutput<T = OutputId> {
    pub extract_from: T,
    pub conditions: Option<Vec<conditions::AnyCondition>>,
}

impl Config {
    pub fn builder() -> builder::ConfigBuilder {
        Default::default()
    }

    /// Expand a logical component id (i.e. from the config file) into the ids of the
    /// components it was expanded to as part of the macro process. Does not check that the
    /// identifier is otherwise valid.
    pub fn get_inputs(&self, identifier: &ComponentKey) -> Vec<ComponentKey> {
        self.expansions
            .get(identifier)
            .cloned()
            .unwrap_or_else(|| vec![identifier.clone()])
    }
}

#[cfg(all(
    test,
    feature = "sources-file",
    feature = "sinks-console",
    feature = "transforms-json_parser"
))]
mod test {
    use std::path::PathBuf;

    use indoc::indoc;

    use super::{builder::ConfigBuilder, format, load_from_str, ComponentKey, Format};

    #[test]
    fn default_data_dir() {
        let config = load_from_str(
            indoc! {r#"
                [sources.in]
                  type = "file"
                  include = ["/var/log/messages"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            Some(PathBuf::from("/var/lib/vector")),
            config.global.data_dir
        )
    }

    #[test]
    fn default_schema() {
        let config = load_from_str(
            indoc! {r#"
                [sources.in]
                  type = "file"
                  include = ["/var/log/messages"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!("host", config.global.log_schema.host_key().to_string());
        assert_eq!(
            "message",
            config.global.log_schema.message_key().to_string()
        );
        assert_eq!(
            "timestamp",
            config.global.log_schema.timestamp_key().to_string()
        );
    }

    #[test]
    fn custom_schema() {
        let config = load_from_str(
            indoc! {r#"
                [log_schema]
                  host_key = "this"
                  message_key = "that"
                  timestamp_key = "then"

                [sources.in]
                  type = "file"
                  include = ["/var/log/messages"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!("this", config.global.log_schema.host_key().to_string());
        assert_eq!("that", config.global.log_schema.message_key().to_string());
        assert_eq!("then", config.global.log_schema.timestamp_key().to_string());
    }

    #[test]
    fn config_append() {
        let mut config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [sources.in]
                  type = "file"
                  include = ["/var/log/messages"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            config.append(
                format::deserialize(
                    indoc! {r#"
                        data_dir = "/foobar"

                        [proxy]
                          http = "http://proxy.inc:3128"

                        [transforms.foo]
                          type = "json_parser"
                          inputs = [ "in" ]

                        [[tests]]
                          name = "check_simple_log"
                          [tests.input]
                            insert_at = "foo"
                            type = "raw"
                            value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"
                          [[tests.outputs]]
                            extract_from = "foo"
                            [[tests.outputs.conditions]]
                              type = "check_fields"
                              "message.equals" = "Sorry, I'm busy this week Cecil"
                    "#},
                    Format::Toml,
                )
                .unwrap()
            ),
            Ok(())
        );

        assert!(config.global.proxy.http.is_some());
        assert!(config.global.proxy.https.is_none());
        assert_eq!(Some(PathBuf::from("/foobar")), config.global.data_dir);
        assert!(config.sources.contains_key(&ComponentKey::from("in")));
        assert!(config.sinks.contains_key(&ComponentKey::from("out")));
        assert!(config.transforms.contains_key(&ComponentKey::from("foo")));
        assert_eq!(config.tests.len(), 1);
    }

    #[test]
    fn config_append_collisions() {
        let mut config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [sources.in]
                  type = "file"
                  include = ["/var/log/messages"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            config.append(
                format::deserialize(
                    indoc! {r#"
                        [sources.in]
                          type = "file"
                          include = ["/var/log/messages"]

                        [transforms.foo]
                          type = "json_parser"
                          inputs = [ "in" ]

                        [sinks.out]
                          type = "console"
                          inputs = ["in"]
                          encoding = "json"
                    "#},
                    Format::Toml,
                )
                .unwrap()
            ),
            Err(vec![
                "duplicate source id found: in".into(),
                "duplicate sink id found: out".into(),
            ])
        );
    }

    #[test]
    fn with_proxy() {
        let config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [proxy]
                  http = "http://server:3128"
                  https = "http://other:3128"
                  no_proxy = ["localhost", "127.0.0.1"]

                [sources.in]
                  type = "nginx_metrics"
                  endpoints = ["http://localhost:8000/basic_status"]
                  proxy.http = "http://server:3128"
                  proxy.https = "http://other:3128"
                  proxy.no_proxy = ["localhost", "127.0.0.1"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();
        assert_eq!(config.global.proxy.http, Some("http://server:3128".into()));
        assert_eq!(config.global.proxy.https, Some("http://other:3128".into()));
        assert!(config.global.proxy.no_proxy.matches("localhost"));
        let source = config.sources.get(&ComponentKey::from("in")).unwrap();
        assert_eq!(source.proxy.http, Some("http://server:3128".into()));
        assert_eq!(source.proxy.https, Some("http://other:3128".into()));
        assert!(source.proxy.no_proxy.matches("localhost"));
    }

    #[test]
    fn with_partial_proxy() {
        let config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [proxy]
                  http = "http://server:3128"

                [sources.in]
                  type = "nginx_metrics"
                  endpoints = ["http://localhost:8000/basic_status"]

                [sources.in.proxy]
                  http = "http://server:3128"
                  https = "http://other:3128"
                  no_proxy = ["localhost", "127.0.0.1"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .unwrap();
        assert_eq!(config.global.proxy.http, Some("http://server:3128".into()));
        assert_eq!(config.global.proxy.https, None);
        let source = config.sources.get(&ComponentKey::from("in")).unwrap();
        assert_eq!(source.proxy.http, Some("http://server:3128".into()));
        assert_eq!(source.proxy.https, Some("http://other:3128".into()));
        assert!(source.proxy.no_proxy.matches("localhost"));
    }

    #[test]
    #[cfg(feature = "datadog-pipelines")]
    fn order_independent_sha256_hashes() {
        let config1: ConfigBuilder = format::deserialize(
            indoc! {r#"
                data_dir = "/tmp"

                [api]
                    enabled = true

                [sources.file]
                    type = "file"
                    ignore_older_secs = 600
                    include = ["/var/log/**/*.log"]
                    read_from = "beginning"

                [sources.internal_metrics]
                    type = "internal_metrics"
                    namespace = "pipelines"

                [transforms.filter]
                    type = "filter"
                    inputs = ["internal_metrics"]
                    condition = """
                        .name == "processed_bytes_total"
                    """

                [sinks.out]
                    type = "console"
                    inputs = ["filter"]
                    target = "stdout"
                    encoding.codec = "json"
            "#},
            Format::Toml,
        )
        .unwrap();

        let config2: ConfigBuilder = format::deserialize(
            indoc! {r#"
                data_dir = "/tmp"

                [sources.internal_metrics]
                    type = "internal_metrics"
                    namespace = "pipelines"

                [sources.file]
                    type = "file"
                    ignore_older_secs = 600
                    include = ["/var/log/**/*.log"]
                    read_from = "beginning"

                [transforms.filter]
                    type = "filter"
                    inputs = ["internal_metrics"]
                    condition = """
                        .name == "processed_bytes_total"
                    """

                [sinks.out]
                    type = "console"
                    inputs = ["filter"]
                    target = "stdout"
                    encoding.codec = "json"

                [api]
                    enabled = true
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(config1.sha256_hash(), config2.sha256_hash())
    }
}

#[cfg(all(test, feature = "sources-stdin", feature = "sinks-console"))]
mod resource_tests {
    use std::{
        collections::{HashMap, HashSet},
        net::{Ipv4Addr, SocketAddr},
    };

    use indoc::indoc;

    use super::{load_from_str, Format, Resource};

    fn localhost(port: u16) -> Resource {
        Resource::tcp(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port))
    }

    fn hashmap(conflicts: Vec<(Resource, Vec<&str>)>) -> HashMap<Resource, HashSet<&str>> {
        conflicts
            .into_iter()
            .map(|(key, values)| (key, values.into_iter().collect()))
            .collect()
    }

    #[test]
    fn valid() {
        let components = vec![
            ("sink_0", vec![localhost(0)]),
            ("sink_1", vec![localhost(1)]),
            ("sink_2", vec![localhost(2)]),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(conflicting, HashMap::new());
    }

    #[test]
    fn conflicting_pair() {
        let components = vec![
            ("sink_0", vec![localhost(0)]),
            ("sink_1", vec![localhost(2)]),
            ("sink_2", vec![localhost(2)]),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(
            conflicting,
            hashmap(vec![(localhost(2), vec!["sink_1", "sink_2"])])
        );
    }

    #[test]
    fn conflicting_multi() {
        let components = vec![
            ("sink_0", vec![localhost(0)]),
            ("sink_1", vec![localhost(2), localhost(0)]),
            ("sink_2", vec![localhost(2)]),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(
            conflicting,
            hashmap(vec![
                (localhost(0), vec!["sink_0", "sink_1"]),
                (localhost(2), vec!["sink_1", "sink_2"])
            ])
        );
    }

    #[test]
    fn different_network_interface() {
        let components = vec![
            ("sink_0", vec![localhost(0)]),
            (
                "sink_1",
                vec![Resource::tcp(SocketAddr::new(
                    Ipv4Addr::new(127, 0, 0, 2).into(),
                    0,
                ))],
            ),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(conflicting, HashMap::new());
    }

    #[test]
    fn unspecified_network_interface() {
        let components = vec![
            ("sink_0", vec![localhost(0)]),
            (
                "sink_1",
                vec![Resource::tcp(SocketAddr::new(
                    Ipv4Addr::UNSPECIFIED.into(),
                    0,
                ))],
            ),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(
            conflicting,
            hashmap(vec![(localhost(0), vec!["sink_0", "sink_1"])])
        );
    }

    #[test]
    fn different_protocol() {
        let components = vec![
            (
                "sink_0",
                vec![Resource::tcp(SocketAddr::new(
                    Ipv4Addr::LOCALHOST.into(),
                    0,
                ))],
            ),
            (
                "sink_1",
                vec![Resource::udp(SocketAddr::new(
                    Ipv4Addr::LOCALHOST.into(),
                    0,
                ))],
            ),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(conflicting, HashMap::new());
    }

    #[test]
    fn config_conflict_detected() {
        assert!(load_from_str(
            indoc! {r#"
                [sources.in0]
                  type = "stdin"

                [sources.in1]
                  type = "stdin"

                [sinks.out]
                  type = "console"
                  inputs = ["in0","in1"]
                  encoding = "json"
            "#},
            Format::Toml,
        )
        .is_err());
    }
}

#[cfg(all(
    test,
    feature = "sources-stdin",
    feature = "sinks-console",
    feature = "transforms-pipelines",
    feature = "transforms-filter"
))]
mod pipelines_tests {
    use indoc::indoc;

    use super::{load_from_str, Format};

    #[test]
    fn forbid_pipeline_nesting() {
        let res = load_from_str(
            indoc! {r#"
                [sources.in]
                  type = "stdin"

                [transforms.processing]
                  inputs = ["in"]
                  type = "pipelines"

                  [transforms.processing.logs.pipelines.foo]
                    name = "foo"

                    [[transforms.processing.logs.pipelines.foo.transforms]]
                      type = "pipelines"

                      [transforms.processing.logs.pipelines.foo.transforms.logs.pipelines.bar]
                        name = "bar"

                          [[transforms.processing.logs.pipelines.foo.transforms.logs.pipelines.bar.transforms]]
                            type = "filter"
                            condition = ""

                [sinks.out]
                  type = "console"
                  inputs = ["processing"]
                  encoding = "json"
            "#},
            Format::Toml,
        );
        assert!(res.is_err(), "should error");
    }
}

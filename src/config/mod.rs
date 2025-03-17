#![allow(missing_docs)]
use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Display, Formatter},
    hash::Hash,
    net::SocketAddr,
    path::PathBuf,
    time::Duration,
};

use crate::{
    conditions,
    event::{Metric, Value},
    secrets::SecretBackends,
    serde::OneOrMany,
};

use indexmap::IndexMap;
use serde::Serialize;

use vector_config::configurable_component;
pub use vector_lib::config::{
    AcknowledgementsConfig, DataType, GlobalOptions, Input, LogNamespace,
    SourceAcknowledgementsConfig, SourceOutput, TransformOutput,
};
pub use vector_lib::configurable::component::{
    GenerateConfig, SinkDescription, TransformDescription,
};

pub mod api;
mod builder;
mod cmd;
mod compiler;
mod diff;
pub mod dot_graph;
mod enrichment_table;
pub mod format;
mod graph;
mod loading;
pub mod provider;
pub mod schema;
mod secret;
mod sink;
mod source;
mod transform;
pub mod unit_test;
mod validation;
mod vars;
pub mod watcher;

pub use builder::ConfigBuilder;
pub use cmd::{cmd, Opts};
pub use diff::ConfigDiff;
pub use enrichment_table::{EnrichmentTableConfig, EnrichmentTableOuter};
pub use format::{Format, FormatHint};
pub use loading::{
    load, load_builder_from_paths, load_from_paths, load_from_paths_with_provider_and_secrets,
    load_from_str, load_source_from_paths, merge_path_lists, process_paths, COLLECTOR,
    CONFIG_PATHS,
};
pub use provider::ProviderConfig;
pub use secret::SecretBackend;
pub use sink::{BoxedSink, SinkConfig, SinkContext, SinkHealthcheckOptions, SinkOuter};
pub use source::{BoxedSource, SourceConfig, SourceContext, SourceOuter};
pub use transform::{
    get_transform_output_ids, BoxedTransform, TransformConfig, TransformContext, TransformOuter,
};
pub use unit_test::{build_unit_tests, build_unit_tests_main, UnitTestResult};
pub use validation::warnings;
pub use vars::{interpolate, ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX};
pub use vector_lib::{
    config::{
        init_log_schema, init_telemetry, log_schema, proxy::ProxyConfig, telemetry, ComponentKey,
        LogSchema, OutputId,
    },
    id::Inputs,
};

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

#[derive(Debug, Default, Serialize)]
pub struct Config {
    #[cfg(feature = "api")]
    pub api: api::Options,
    pub schema: schema::Options,
    pub global: GlobalOptions,
    pub healthchecks: HealthcheckOptions,
    sources: IndexMap<ComponentKey, SourceOuter>,
    sinks: IndexMap<ComponentKey, SinkOuter<OutputId>>,
    transforms: IndexMap<ComponentKey, TransformOuter<OutputId>>,
    pub enrichment_tables: IndexMap<ComponentKey, EnrichmentTableOuter<OutputId>>,
    tests: Vec<TestDefinition>,
    secret: IndexMap<ComponentKey, SecretBackends>,
    pub graceful_shutdown_duration: Option<Duration>,
}

impl Config {
    pub fn builder() -> builder::ConfigBuilder {
        Default::default()
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn sources(&self) -> impl Iterator<Item = (&ComponentKey, &SourceOuter)> {
        self.sources.iter()
    }

    pub fn source(&self, id: &ComponentKey) -> Option<&SourceOuter> {
        self.sources.get(id)
    }

    pub fn transforms(&self) -> impl Iterator<Item = (&ComponentKey, &TransformOuter<OutputId>)> {
        self.transforms.iter()
    }

    pub fn transform(&self, id: &ComponentKey) -> Option<&TransformOuter<OutputId>> {
        self.transforms.get(id)
    }

    pub fn sinks(&self) -> impl Iterator<Item = (&ComponentKey, &SinkOuter<OutputId>)> {
        self.sinks.iter()
    }

    pub fn sink(&self, id: &ComponentKey) -> Option<&SinkOuter<OutputId>> {
        self.sinks.get(id)
    }

    pub fn enrichment_tables(
        &self,
    ) -> impl Iterator<Item = (&ComponentKey, &EnrichmentTableOuter<OutputId>)> {
        self.enrichment_tables.iter()
    }

    pub fn enrichment_table(&self, id: &ComponentKey) -> Option<&EnrichmentTableOuter<OutputId>> {
        self.enrichment_tables.get(id)
    }

    pub fn inputs_for_node(&self, id: &ComponentKey) -> Option<&[OutputId]> {
        self.transforms
            .get(id)
            .map(|t| &t.inputs[..])
            .or_else(|| self.sinks.get(id).map(|s| &s.inputs[..]))
            .or_else(|| self.enrichment_tables.get(id).map(|s| &s.inputs[..]))
    }

    pub fn propagate_acknowledgements(&mut self) -> Result<(), Vec<String>> {
        let inputs: Vec<_> = self
            .sinks
            .iter()
            .filter(|(_, sink)| {
                sink.inner
                    .acknowledgements()
                    .merge_default(&self.global.acknowledgements)
                    .enabled()
            })
            .flat_map(|(name, sink)| {
                sink.inputs
                    .iter()
                    .map(|input| (name.clone(), input.clone()))
            })
            .collect();
        self.propagate_acks_rec(inputs);
        Ok(())
    }

    fn propagate_acks_rec(&mut self, sink_inputs: Vec<(ComponentKey, OutputId)>) {
        for (sink, input) in sink_inputs {
            let component = &input.component;
            if let Some(source) = self.sources.get_mut(component) {
                if source.inner.can_acknowledge() {
                    source.sink_acknowledgements = true;
                } else {
                    warn!(
                        message = "Source has acknowledgements enabled by a sink, but acknowledgements are not supported by this source. Silent data loss could occur.",
                        source = component.id(),
                        sink = sink.id(),
                    );
                }
            } else if let Some(transform) = self.transforms.get(component) {
                let inputs = transform
                    .inputs
                    .iter()
                    .map(|input| (sink.clone(), input.clone()))
                    .collect();
                self.propagate_acks_rec(inputs);
            }
        }
    }
}

/// Healthcheck options.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(default)]
pub struct HealthcheckOptions {
    /// Whether or not healthchecks are enabled for all sinks.
    ///
    /// Can be overridden on a per-sink basis.
    pub enabled: bool,

    /// Whether or not to require a sink to report as being healthy during startup.
    ///
    /// When enabled and a sink reports not being healthy, Vector will exit during start-up.
    ///
    /// Can be alternatively set, and overridden by, the `--require-healthy` command-line flag.
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

/// Unique thing, like port, of which only one owner can be.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Resource {
    Port(SocketAddr, Protocol),
    SystemFdOffset(usize),
    Fd(u32),
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
                        unspecified.push((key.clone(), *address, *protocol));
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
        for (key, address0, protocol0) in unspecified {
            for (resource, components) in resource_map.iter_mut() {
                if let Resource::Port(address, protocol) = resource {
                    // IP addresses can either be v4 or v6.
                    // Therefore we check if the ip version matches, the port matches and if the protocol (TCP/UDP) matches
                    // when checking for equality.
                    if &address0 == address && &protocol0 == protocol {
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
            Resource::Fd(fd) => write!(fmt, "file descriptor: {}", fd),
            Resource::DiskBuffer(name) => write!(fmt, "disk buffer {:?}", name),
        }
    }
}

/// A unit test definition.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestDefinition<T: 'static = OutputId> {
    /// The name of the unit test.
    pub name: String,

    /// An input event to test against.
    pub input: Option<TestInput>,

    /// A set of input events to test against.
    #[serde(default)]
    pub inputs: Vec<TestInput>,

    /// A set of expected output events after the test has run.
    #[serde(default)]
    pub outputs: Vec<TestOutput<T>>,

    /// A set of component outputs that should not have emitted any events.
    #[serde(default)]
    pub no_outputs_from: Vec<T>,
}

impl TestDefinition<String> {
    fn resolve_outputs(
        self,
        graph: &graph::Graph,
    ) -> Result<TestDefinition<OutputId>, Vec<String>> {
        let TestDefinition {
            name,
            input,
            inputs,
            outputs,
            no_outputs_from,
        } = self;
        let mut errors = Vec::new();

        let output_map = graph.input_map().expect("ambiguous outputs");

        let outputs = outputs
            .into_iter()
            .map(|old| {
                let TestOutput {
                    extract_from,
                    conditions,
                } = old;

                (extract_from.to_vec(), conditions)
            })
            .filter_map(|(extract_from, conditions)| {
                let mut outputs = Vec::new();
                for from in extract_from {
                    if let Some(output_id) = output_map.get(&from) {
                        outputs.push(output_id.clone());
                    } else {
                        errors.push(format!(
                            r#"Invalid extract_from target in test '{}': '{}' does not exist"#,
                            name, from
                        ));
                    }
                }
                if outputs.is_empty() {
                    None
                } else {
                    Some(TestOutput {
                        extract_from: outputs.into(),
                        conditions,
                    })
                }
            })
            .collect();

        let no_outputs_from = no_outputs_from
            .into_iter()
            .filter_map(|o| {
                if let Some(output_id) = output_map.get(&o) {
                    Some(output_id.clone())
                } else {
                    errors.push(format!(
                        r#"Invalid no_outputs_from target in test '{}': '{}' does not exist"#,
                        name, o
                    ));
                    None
                }
            })
            .collect();

        if errors.is_empty() {
            Ok(TestDefinition {
                name,
                input,
                inputs,
                outputs,
                no_outputs_from,
            })
        } else {
            Err(errors)
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
                extract_from: old
                    .extract_from
                    .to_vec()
                    .into_iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
                    .into(),
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

/// A unit test input.
///
/// An input describes not only the type of event to insert, but also which transform within the
/// configuration to insert it to.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestInput {
    /// The name of the transform to insert the input event to.
    pub insert_at: ComponentKey,

    /// The type of the input event.
    ///
    /// Can be either `raw`, `vrl`, `log`, or `metric.
    #[serde(default = "default_test_input_type", rename = "type")]
    pub type_str: String,

    /// The raw string value to use as the input event.
    ///
    /// Use this only when the input event should be a raw event (i.e. unprocessed/undecoded log
    /// event) and when the input type is set to `raw`.
    pub value: Option<String>,

    /// The vrl expression to generate the input event.
    ///
    /// Only relevant when `type` is `vrl`.
    pub source: Option<String>,

    /// The set of log fields to use when creating a log input event.
    ///
    /// Only relevant when `type` is `log`.
    pub log_fields: Option<IndexMap<String, Value>>,

    /// The metric to use as an input event.
    ///
    /// Only relevant when `type` is `metric`.
    pub metric: Option<Metric>,
}

fn default_test_input_type() -> String {
    "raw".to_string()
}

/// A unit test output.
///
/// An output describes what we expect a transform to emit when fed a certain event, or events, when
/// running a unit test.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestOutput<T: 'static = OutputId> {
    /// The transform outputs to extract events from.
    pub extract_from: OneOrMany<T>,

    /// The conditions to run against the output to validate that they were transformed as expected.
    pub conditions: Option<Vec<conditions::AnyCondition>>,
}

#[cfg(all(test, feature = "sources-file", feature = "sinks-console"))]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{config, topology};
    use indoc::indoc;

    use super::{builder::ConfigBuilder, format, load_from_str, ComponentKey, ConfigDiff, Format};

    async fn load(config: &str, format: config::Format) -> Result<Vec<String>, Vec<String>> {
        match config::load_from_str(config, format) {
            Ok(c) => {
                let diff = ConfigDiff::initial(&c);
                let c2 = config::load_from_str(config, format).unwrap();
                match (
                    config::warnings(&c2),
                    topology::TopologyPieces::build(&c, &diff, HashMap::new(), Default::default())
                        .await,
                ) {
                    (warnings, Ok(_pieces)) => Ok(warnings),
                    (_, Err(errors)) => Err(errors),
                }
            }
            Err(error) => Err(error),
        }
    }

    #[tokio::test]
    async fn bad_inputs() {
        let err = load(
            r#"
            [sources.in]
            type = "test_basic"

            [transforms.sample]
            type = "test_basic"
            inputs = []
            suffix = "foo"
            increase = 1.25

            [transforms.sample2]
            type = "test_basic"
            inputs = ["qwerty"]
            suffix = "foo"
            increase = 1.25

            [sinks.out]
            type = "test_basic"
            inputs = ["asdf", "in", "in"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap_err();

        assert_eq!(
            vec![
                "Sink \"out\" has input \"in\" duplicated 2 times",
                "Transform \"sample\" has no inputs",
                "Input \"qwerty\" for transform \"sample2\" doesn't match any components.",
                "Input \"asdf\" for sink \"out\" doesn't match any components.",
            ],
            err,
        );
    }

    #[tokio::test]
    async fn duplicate_name() {
        let err = load(
            r#"
            [sources.foo]
            type = "test_basic"

            [sources.bar]
            type = "test_basic"

            [transforms.foo]
            type = "test_basic"
            inputs = ["bar"]
            suffix = "foo"
            increase = 1.25

            [sinks.out]
            type = "test_basic"
            inputs = ["foo"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap_err();

        assert_eq!(
            err,
            vec!["More than one component with name \"foo\" (source, transform).",]
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn conflicting_stdin_and_fd_resources() {
        let errors = load(
            r#"
            [sources.stdin]
            type = "stdin"

            [sources.file_descriptor]
            type = "file_descriptor"
            fd = 0

            [sinks.out]
            type = "test_basic"
            inputs = ["stdin", "file_descriptor"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap_err();

        assert_eq!(errors.len(), 1);
        let expected_prefix = "Resource `file descriptor: 0` is claimed by multiple components:";
        assert!(errors[0].starts_with(expected_prefix));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn conflicting_fd_resources() {
        let errors = load(
            r#"
            [sources.file_descriptor1]
            type = "file_descriptor"
            fd = 10
            [sources.file_descriptor2]
            type = "file_descriptor"
            fd = 10
            [sinks.out]
            type = "test_basic"
            inputs = ["file_descriptor1", "file_descriptor2"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap_err();

        assert_eq!(errors.len(), 1);
        let expected_prefix = "Resource `file descriptor: 10` is claimed by multiple components:";
        assert!(errors[0].starts_with(expected_prefix));
    }

    #[tokio::test]
    #[cfg(all(unix, feature = "sources-file_descriptor"))]
    async fn no_conflict_fd_resources() {
        use crate::sources::file_descriptors::file_descriptor::null_fd;
        let fd1 = null_fd().unwrap();
        let fd2 = null_fd().unwrap();
        let result = load(
            &format!(
                r#"
            [sources.file_descriptor1]
            type = "file_descriptor"
            fd = {fd1}

            [sources.file_descriptor2]
            type = "file_descriptor"
            fd = {fd2}

            [sinks.out]
            type = "test_basic"
            inputs = ["file_descriptor1", "file_descriptor2"]
            "#
            ),
            Format::Toml,
        )
        .await;

        let expected = Ok(vec![]);
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn warnings() {
        let warnings = load(
            r#"
            [sources.in1]
            type = "test_basic"

            [sources.in2]
            type = "test_basic"

            [transforms.sample1]
            type = "test_basic"
            inputs = ["in1"]
            suffix = "foo"
            increase = 1.25

            [transforms.sample2]
            type = "test_basic"
            inputs = ["in1"]
            suffix = "foo"
            increase = 1.25

            [sinks.out]
            type = "test_basic"
            inputs = ["sample1"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap();

        assert_eq!(
            warnings,
            vec![
                "Transform \"sample2\" has no consumers",
                "Source \"in2\" has no consumers",
            ]
        )
    }

    #[tokio::test]
    async fn cycle() {
        let errors = load(
            r#"
            [sources.in]
            type = "test_basic"

            [transforms.one]
            type = "test_basic"
            inputs = ["in"]
            suffix = "foo"
            increase = 1.25

            [transforms.two]
            type = "test_basic"
            inputs = ["one", "four"]
            suffix = "foo"
            increase = 1.25

            [transforms.three]
            type = "test_basic"
            inputs = ["two"]
            suffix = "foo"
            increase = 1.25

            [transforms.four]
            type = "test_basic"
            inputs = ["three"]
            suffix = "foo"
            increase = 1.25

            [sinks.out]
            type = "test_basic"
            inputs = ["four"]
            "#,
            Format::Toml,
        )
        .await
        .unwrap_err();

        assert_eq!(
            errors,
            vec!["Cyclic dependency detected in the chain [ four -> two -> three -> four ]"]
        )
    }

    #[test]
    fn default_data_dir() {
        let config = load_from_str(
            indoc! {r#"
                [sources.in]
                type = "test_basic"

                [sinks.out]
                type = "test_basic"
                inputs = ["in"]
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
            type = "test_basic"

            [sinks.out]
            type = "test_basic"
            inputs = ["in"]
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            "host",
            config.global.log_schema.host_key().unwrap().to_string()
        );
        assert_eq!(
            "message",
            config.global.log_schema.message_key().unwrap().to_string()
        );
        assert_eq!(
            "timestamp",
            config
                .global
                .log_schema
                .timestamp_key()
                .unwrap()
                .to_string()
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
                  type = "test_basic"

                [sinks.out]
                  type = "test_basic"
                  inputs = ["in"]
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            "this",
            config.global.log_schema.host_key().unwrap().to_string()
        );
        assert_eq!(
            "that",
            config.global.log_schema.message_key().unwrap().to_string()
        );
        assert_eq!(
            "then",
            config
                .global
                .log_schema
                .timestamp_key()
                .unwrap()
                .to_string()
        );
    }

    #[test]
    fn config_append() {
        let mut config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [sources.in]
                  type = "test_basic"

                [sinks.out]
                  type = "test_basic"
                  inputs = ["in"]
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
                          type = "test_basic"
                          inputs = [ "in" ]
                          suffix = "foo"
                          increase = 1.25

                        [[tests]]
                          name = "check_simple_log"
                          [tests.input]
                            insert_at = "foo"
                            type = "raw"
                            value = "2019-11-28T12:00:00+00:00 info Sorry, I'm busy this week Cecil"
                          [[tests.outputs]]
                            extract_from = "foo"
                            [[tests.outputs.conditions]]
                              type = "vrl"
                              source = ".message == \"Sorry, I'm busy this week Cecil\""
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
                  type = "test_basic"

                [sinks.out]
                  type = "test_basic"
                  inputs = ["in"]
            "#},
            Format::Toml,
        )
        .unwrap();

        assert_eq!(
            config.append(
                format::deserialize(
                    indoc! {r#"
                        [sources.in]
                          type = "test_basic"

                        [transforms.foo]
                          type = "test_basic"
                          inputs = [ "in" ]
                          suffix = "foo"
                          increase = 1.25

                        [sinks.out]
                          type = "test_basic"
                          inputs = ["in"]
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
                  encoding.codec = "json"
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
    fn with_partial_global_proxy() {
        let config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [proxy]
                  http = "http://server:3128"

                [sources.in]
                  type = "nginx_metrics"
                  endpoints = ["http://localhost:8000/basic_status"]

                [sources.in.proxy]
                  http = "http://server:3129"
                  https = "http://other:3129"
                  no_proxy = ["localhost", "127.0.0.1"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding.codec = "json"
            "#},
            Format::Toml,
        )
        .unwrap();
        assert_eq!(config.global.proxy.http, Some("http://server:3128".into()));
        assert_eq!(config.global.proxy.https, None);
        let source = config.sources.get(&ComponentKey::from("in")).unwrap();
        assert_eq!(source.proxy.http, Some("http://server:3129".into()));
        assert_eq!(source.proxy.https, Some("http://other:3129".into()));
        assert!(source.proxy.no_proxy.matches("localhost"));
    }

    #[test]
    fn with_partial_source_proxy() {
        let config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                [proxy]
                  http = "http://server:3128"
                  https = "http://other:3128"

                [sources.in]
                  type = "nginx_metrics"
                  endpoints = ["http://localhost:8000/basic_status"]

                [sources.in.proxy]
                  http = "http://server:3129"
                  no_proxy = ["localhost", "127.0.0.1"]

                [sinks.out]
                  type = "console"
                  inputs = ["in"]
                  encoding.codec = "json"
            "#},
            Format::Toml,
        )
        .unwrap();
        assert_eq!(config.global.proxy.http, Some("http://server:3128".into()));
        assert_eq!(config.global.proxy.https, Some("http://other:3128".into()));
        let source = config.sources.get(&ComponentKey::from("in")).unwrap();
        assert_eq!(source.proxy.http, Some("http://server:3129".into()));
        assert_eq!(source.proxy.https, None);
        assert!(source.proxy.no_proxy.matches("localhost"));
    }
}

#[cfg(all(test, feature = "sources-file", feature = "sinks-file"))]
mod acknowledgements_tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn propagates_settings() {
        // The topology:
        // in1 => out1
        // in2 => out2 (acks enabled)
        // in3 => parse3 => out3 (acks enabled)
        let config: ConfigBuilder = format::deserialize(
            indoc! {r#"
                data_dir = "/tmp"
                [sources.in1]
                    type = "file"
                    include = ["/var/log/**/*.log"]
                [sources.in2]
                    type = "file"
                    include = ["/var/log/**/*.log"]
                [sources.in3]
                    type = "file"
                    include = ["/var/log/**/*.log"]
                [transforms.parse3]
                    type = "test_basic"
                    inputs = ["in3"]
                    increase = 0.0
                    suffix = ""
                [sinks.out1]
                    type = "file"
                    inputs = ["in1"]
                    encoding.codec = "text"
                    path = "/path/to/out1"
                [sinks.out2]
                    type = "file"
                    inputs = ["in2"]
                    encoding.codec = "text"
                    path = "/path/to/out2"
                    acknowledgements = true
                [sinks.out3]
                    type = "file"
                    inputs = ["parse3"]
                    encoding.codec = "text"
                    path = "/path/to/out3"
                    acknowledgements.enabled = true
            "#},
            Format::Toml,
        )
        .unwrap();

        for source in config.sources.values() {
            assert!(
                !source.sink_acknowledgements,
                "Source `sink_acknowledgements` should be `false` before propagation"
            );
        }

        let config = config.build().unwrap();

        let get = |key: &str| config.sources.get(&ComponentKey::from(key)).unwrap();
        assert!(!get("in1").sink_acknowledgements);
        assert!(get("in2").sink_acknowledgements);
        assert!(get("in3").sink_acknowledgements);
    }
}

#[cfg(test)]
mod resource_tests {
    use std::{
        collections::{HashMap, HashSet},
        net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    };

    use proptest::prelude::*;

    use super::Resource;

    fn tcp(addr: impl Into<IpAddr>, port: u16) -> Resource {
        Resource::tcp(SocketAddr::new(addr.into(), port))
    }

    fn udp(addr: impl Into<IpAddr>, port: u16) -> Resource {
        Resource::udp(SocketAddr::new(addr.into(), port))
    }

    fn unspecified() -> impl Strategy<Value = IpAddr> {
        prop_oneof![
            Just(Ipv4Addr::UNSPECIFIED.into()),
            Just(Ipv6Addr::UNSPECIFIED.into()),
        ]
    }

    fn specaddr() -> impl Strategy<Value = IpAddr> {
        any::<IpAddr>().prop_filter("Must be specific address", |addr| !addr.is_unspecified())
    }

    fn specport() -> impl Strategy<Value = u16> {
        any::<u16>().prop_filter("Must be specific port", |&port| port > 0)
    }

    fn hashmap(conflicts: Vec<(Resource, Vec<&str>)>) -> HashMap<Resource, HashSet<&str>> {
        conflicts
            .into_iter()
            .map(|(key, values)| (key, values.into_iter().collect()))
            .collect()
    }

    proptest! {
        #[test]
        fn valid(addr: IpAddr, port1 in specport(), port2 in specport()) {
            prop_assume!(port1 != port2);
            let components = vec![
                ("sink_0", vec![tcp(addr, 0)]),
                ("sink_1", vec![tcp(addr, port1)]),
                ("sink_2", vec![tcp(addr, port2)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(conflicting, HashMap::new());
        }

        #[test]
        fn conflicting_pair(addr: IpAddr, port in specport()) {
            let components = vec![
                ("sink_0", vec![tcp(addr, 0)]),
                ("sink_1", vec![tcp(addr, port)]),
                ("sink_2", vec![tcp(addr, port)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(
                conflicting,
                hashmap(vec![(tcp(addr, port), vec!["sink_1", "sink_2"])])
            );
        }

        #[test]
        fn conflicting_multi(addr: IpAddr, port in specport()) {
            let components = vec![
                ("sink_0", vec![tcp(addr, 0)]),
                ("sink_1", vec![tcp(addr, port), tcp(addr, 0)]),
                ("sink_2", vec![tcp(addr, port)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(
                conflicting,
                hashmap(vec![
                    (tcp(addr, 0), vec!["sink_0", "sink_1"]),
                    (tcp(addr, port), vec!["sink_1", "sink_2"])
                ])
            );
        }

        #[test]
        fn different_network_interface(addr1: IpAddr, addr2: IpAddr, port: u16) {
            prop_assume!(addr1 != addr2);
            let components = vec![
                ("sink_0", vec![tcp(addr1, port)]),
                ("sink_1", vec![tcp(addr2, port)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(conflicting, HashMap::new());
        }

        #[test]
        fn unspecified_network_interface(addr in specaddr(), unspec in unspecified(), port: u16) {
            let components = vec![
                ("sink_0", vec![tcp(addr, port)]),
                ("sink_1", vec![tcp(unspec, port)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(conflicting, HashMap::new());
        }

        #[test]
        fn different_protocol(addr: IpAddr) {
            let components = vec![
                ("sink_0", vec![tcp(addr, 0)]),
                ("sink_1", vec![udp(addr, 0)]),
            ];
            let conflicting = Resource::conflicts(components);
            assert_eq!(conflicting, HashMap::new());
        }
    }

    #[test]
    fn different_unspecified_ip_version() {
        let components = vec![
            ("sink_0", vec![tcp(Ipv4Addr::UNSPECIFIED, 0)]),
            ("sink_1", vec![tcp(Ipv6Addr::UNSPECIFIED, 0)]),
        ];
        let conflicting = Resource::conflicts(components);
        assert_eq!(conflicting, HashMap::new());
    }
}

#[cfg(all(test, feature = "sources-stdin", feature = "sinks-console"))]
mod resource_config_tests {
    use indoc::indoc;
    use vector_lib::configurable::schema::generate_root_schema;

    use super::{load_from_str, Format};

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
                  encoding.codec = "json"
            "#},
            Format::Toml,
        )
        .is_err());
    }

    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    #[allow(clippy::print_stderr)]
    fn generate_component_config_schema() {
        use crate::config::{SinkOuter, SourceOuter, TransformOuter};
        use indexmap::IndexMap;
        use vector_lib::config::ComponentKey;
        use vector_lib::configurable::configurable_component;

        /// Top-level Vector configuration.
        #[configurable_component]
        #[derive(Clone)]
        struct ComponentsOnlyConfig {
            /// Configured sources.
            #[serde(default)]
            pub sources: IndexMap<ComponentKey, SourceOuter>,

            /// Configured transforms.
            #[serde(default)]
            pub transforms: IndexMap<ComponentKey, TransformOuter<String>>,

            /// Configured sinks.
            #[serde(default)]
            pub sinks: IndexMap<ComponentKey, SinkOuter<String>>,
        }

        match generate_root_schema::<ComponentsOnlyConfig>() {
            Ok(schema) => {
                let json = serde_json::to_string_pretty(&schema)
                    .expect("rendering root schema to JSON should not fail");

                println!("{}", json);
            }
            Err(e) => eprintln!("error while generating schema: {:?}", e),
        }
    }
}

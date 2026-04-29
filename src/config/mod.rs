#![allow(missing_docs)]
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt::{self, Display, Formatter},
    fs,
    hash::Hash,
    net::SocketAddr,
    path::PathBuf,
    time::Duration,
};

use indexmap::IndexMap;
use serde::Serialize;
use vector_config::configurable_component;
pub use vector_lib::{
    config::{
        AcknowledgementsConfig, DataType, GlobalOptions, Input, LogNamespace,
        SourceAcknowledgementsConfig, SourceOutput, TransformOutput, WildcardMatching,
    },
    configurable::component::{GenerateConfig, SinkDescription, TransformDescription},
};

use crate::{
    conditions,
    event::{Metric, Value},
    secrets::SecretBackends,
    serde::OneOrMany,
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
pub mod loading;
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
pub use cmd::{Opts, cmd};
pub use diff::ConfigDiff;
pub use enrichment_table::{EnrichmentTableConfig, EnrichmentTableOuter};
pub use format::{Format, FormatHint};
pub use loading::{
    COLLECTOR, CONFIG_PATHS, load, load_from_paths, load_from_paths_with_provider_and_secrets,
    load_from_str, load_from_str_with_secrets, load_source_from_paths, merge_path_lists,
    process_paths,
};
pub use provider::ProviderConfig;
pub use secret::SecretBackend;
pub use sink::{BoxedSink, SinkConfig, SinkContext, SinkHealthcheckOptions, SinkOuter};
pub use source::{BoxedSource, SourceConfig, SourceContext, SourceOuter};
pub use transform::{
    BoxedTransform, TransformConfig, TransformContext, TransformOuter, get_transform_output_ids,
};
pub use unit_test::{UnitTestResult, build_unit_tests, build_unit_tests_main};
pub use validation::warnings;
pub use vars::{ENVIRONMENT_VARIABLE_INTERPOLATION_REGEX, interpolate};
pub use vector_lib::{
    config::{
        ComponentKey, LogSchema, OutputId, init_log_schema, init_telemetry, log_schema,
        proxy::ProxyConfig, telemetry,
    },
    id::Inputs,
};

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
// // This is not a comprehensive set; variants are added as needed.
pub enum ComponentType {
    Transform,
    Sink,
    EnrichmentTable,
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct ComponentConfig {
    pub config_paths: Vec<PathBuf>,
    pub component_key: ComponentKey,
    pub component_type: ComponentType,
}

impl ComponentConfig {
    pub fn new(
        config_paths: Vec<PathBuf>,
        component_key: ComponentKey,
        component_type: ComponentType,
    ) -> Self {
        let canonicalized_paths = config_paths
            .into_iter()
            .filter_map(|p| fs::canonicalize(p).ok())
            .collect();

        Self {
            config_paths: canonicalized_paths,
            component_key,
            component_type,
        }
    }

    pub fn contains(
        &self,
        config_paths: &HashSet<PathBuf>,
    ) -> Option<(ComponentKey, ComponentType)> {
        if config_paths.iter().any(|p| self.config_paths.contains(p)) {
            return Some((self.component_key.clone(), self.component_type.clone()));
        }
        None
    }
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

    pub fn transform_keys_with_external_files(&self) -> HashSet<ComponentKey> {
        self.transforms
            .iter()
            .filter_map(|(name, transform_outer)| {
                if !transform_outer.inner.files_to_watch().is_empty() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Compute which components are "on authoritative paths."
    ///
    /// A component is on an authoritative path if it is an authoritative sink,
    /// or a transform/source that transitively feeds into at least one
    /// authoritative sink.
    ///
    /// Returns `None` if no sink has `authoritative: true` explicitly set,
    /// meaning the feature is inactive and all sinks participate in acks
    /// (preserving backwards compatibility).
    pub fn compute_authoritative_components(&self) -> Option<HashSet<ComponentKey>> {
        // Step 1: Check if ANY sink is explicitly authoritative AND has acks enabled.
        // Both conditions are required: `authoritative` without `enabled` is a
        // misconfiguration that should not activate the stripping feature.
        let any_explicitly_authoritative = self.sinks.iter().any(|(_, sink)| {
            let acks = sink
                .inner
                .acknowledgements()
                .merge_default(&self.global.acknowledgements);
            acks.is_explicitly_authoritative() && acks.enabled()
        });

        if !any_explicitly_authoritative {
            return None; // Feature inactive; all sinks participate
        }

        // Step 2: Collect all authoritative sinks.
        // A sink must have both `enabled` AND `authoritative` to be included.
        // `enabled` gates source-level ack propagation via propagate_acks_rec;
        // a sink with `authoritative: true` but `enabled: false` would activate
        // stripping for other sinks without actually participating in acks.
        let authoritative_sinks: HashSet<ComponentKey> = self
            .sinks
            .iter()
            .filter(|(_, sink)| {
                let acks = sink
                    .inner
                    .acknowledgements()
                    .merge_default(&self.global.acknowledgements);
                acks.enabled() && acks.authoritative()
            })
            .map(|(key, _)| key.clone())
            .collect();

        // Step 3: BFS backward from authoritative sinks through edges.
        //
        // `inputs_for_node` returns `OutputId`s whose `component` field is the
        // upstream component key. Named outputs (routes) produce `OutputId`s
        // with different `port` values but the same `component`, so the BFS
        // correctly marks the upstream component as authoritative regardless of
        // which specific route feeds into the current node.
        let mut on_authoritative_path: HashSet<ComponentKey> = authoritative_sinks.clone();
        let mut queue: VecDeque<ComponentKey> = authoritative_sinks.into_iter().collect();

        while let Some(component) = queue.pop_front() {
            if let Some(inputs) = self.inputs_for_node(&component) {
                for input in inputs {
                    let upstream = &input.component;
                    if on_authoritative_path.insert(upstream.clone()) {
                        queue.push_back(upstream.clone());
                    }
                }
            }
        }

        // NOTE: We intentionally do NOT do a forward sweep to add components
        // from non-authoritative sources. Instead, the runtime uses per-edge
        // strip decisions: strip only when the upstream IS in this set and the
        // downstream is NOT. This naturally preserves legacy behavior for
        // pipelines whose sources have no authoritative sinks downstream.

        Some(on_authoritative_path)
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

    const fn merge(&mut self, other: Self) {
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

impl_generate_config_from_default!(HealthcheckOptions);

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
                if let Resource::Port(address, protocol) = &resource
                    && address.ip().is_unspecified()
                {
                    unspecified.push((key.clone(), *address, *protocol));
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
            Resource::Port(address, protocol) => write!(fmt, "{protocol} {address}"),
            Resource::SystemFdOffset(offset) => write!(fmt, "systemd {}th socket", offset + 1),
            Resource::Fd(fd) => write!(fmt, "file descriptor: {fd}"),
            Resource::DiskBuffer(name) => write!(fmt, "disk buffer {name:?}"),
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
                    expected_event_count,
                } = old;

                (extract_from.to_vec(), conditions, expected_event_count)
            })
            .filter_map(|(extract_from, conditions, expected_event_count)| {
                let mut outputs = Vec::new();
                for from in extract_from {
                    if no_outputs_from.contains(&from) {
                        errors.push(format!(
                            r#"Invalid extract_from target in test '{name}': '{from}' listed in no_outputs_from"#
                        ));
                    } else if let Some(output_id) = output_map.get(&from) {
                        outputs.push(output_id.clone());
                    } else {
                        errors.push(format!(
                            r#"Invalid extract_from target in test '{name}': '{from}' does not exist"#
                        ));
                    }
                }
                if outputs.is_empty() {
                    None
                } else {
                    Some(TestOutput {
                        extract_from: outputs.into(),
                        conditions,
                        expected_event_count,
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
                        r#"Invalid no_outputs_from target in test '{name}': '{o}' does not exist"#
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
                expected_event_count: old.expected_event_count,
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

    /// The expected number of events to be produced by the transform.
    ///
    /// If specified, the test will fail if the number of events emitted by the
    /// transform does not match this value. This check is independent of
    /// `conditions` -- the count is verified first, then each condition is
    /// evaluated against the output events separately. This is useful for
    /// transforms that may emit multiple events.
    pub expected_event_count: Option<usize>,
}

#[cfg(all(test, feature = "sources-file", feature = "sinks-console"))]
mod tests {
    use std::path::PathBuf;

    use indoc::indoc;

    use super::{ComponentKey, ConfigDiff, Format, builder::ConfigBuilder, format, load_from_str};
    use crate::{config, topology::builder::TopologyPiecesBuilder};

    async fn load(config: &str, format: config::Format) -> Result<Vec<String>, Vec<String>> {
        match config::load_from_str(config, format) {
            Ok(c) => {
                let diff = ConfigDiff::initial(&c);
                let c2 = config::load_from_str(config, format).unwrap();
                match (
                    config::warnings(&c2),
                    TopologyPiecesBuilder::new(&c, &diff).build().await,
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

    use super::{Format, load_from_str};

    #[test]
    fn config_conflict_detected() {
        assert!(
            load_from_str(
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
            .is_err()
        );
    }

    #[test]
    #[ignore]
    #[allow(clippy::print_stdout)]
    #[allow(clippy::print_stderr)]
    fn generate_component_config_schema() {
        use indexmap::IndexMap;
        use vector_lib::{config::ComponentKey, configurable::configurable_component};

        use crate::config::{SinkOuter, SourceOuter, TransformOuter};

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

                println!("{json}");
            }
            Err(e) => eprintln!("error while generating schema: {e:?}"),
        }
    }
}

#[cfg(test)]
mod authoritative_tests {
    use std::collections::HashSet;

    use vector_lib::config::{AcknowledgementsConfig, ComponentKey};

    use super::builder::ConfigBuilder;
    use crate::test_util::mock::{basic_sink, basic_sink_with_acks, basic_source, basic_transform};

    /// Helper to create an `AcknowledgementsConfig` with specific enabled and authoritative values.
    fn ack_config(enabled: bool, authoritative: bool) -> AcknowledgementsConfig {
        AcknowledgementsConfig::new(Some(enabled), Some(authoritative))
    }

    #[test]
    fn returns_none_when_no_sink_is_explicitly_authoritative() {
        // Build a config where sinks have acks enabled but no authoritative: true.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_sink(
            "sink_acks_enabled",
            &["src"],
            basic_sink_with_acks(10, AcknowledgementsConfig::from(true)).1,
        );
        config.add_sink("sink_default", &["src"], basic_sink(10).1);

        let config = config.build().unwrap();
        assert!(
            config.compute_authoritative_components().is_none(),
            "Should return None when no sink has authoritative: true"
        );
    }

    #[test]
    fn returns_none_for_empty_config() {
        let mut config = ConfigBuilder::default();
        config.allow_empty = true;
        let config = config.build().unwrap();
        assert!(
            config.compute_authoritative_components().is_none(),
            "Should return None for empty config"
        );
    }

    #[test]
    fn linear_pipeline_returns_all_components() {
        // source -> transform -> auth_sink
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_transform("xform", &["src"], basic_transform("", 0.0));
        config.add_sink(
            "auth_sink",
            &["xform"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );

        let config = config.build().unwrap();
        let result = config.compute_authoritative_components();
        assert!(
            result.is_some(),
            "Should return Some when authoritative sink exists"
        );

        let components = result.unwrap();
        let expected: HashSet<ComponentKey> = [
            ComponentKey::from("src"),
            ComponentKey::from("xform"),
            ComponentKey::from("auth_sink"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            components, expected,
            "Should include source, transform, and authoritative sink"
        );
    }

    #[test]
    fn fan_out_marks_shared_upstream_as_authoritative() {
        // source -> transform -> auth_sink
        //                     -> non_auth_sink
        //
        // The transform and source feed auth_sink, so they are on the authoritative path.
        // non_auth_sink is not authoritative but its upstream components (source, transform)
        // are on the authoritative path because they also feed auth_sink.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_transform("xform", &["src"], basic_transform("", 0.0));
        config.add_sink(
            "auth_sink",
            &["xform"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );
        config.add_sink(
            "non_auth_sink",
            &["xform"],
            basic_sink_with_acks(10, ack_config(true, false)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some");

        // The authoritative path includes: src, xform, auth_sink
        // non_auth_sink is NOT on the authoritative path
        assert!(
            components.contains(&ComponentKey::from("src")),
            "Source should be on authoritative path"
        );
        assert!(
            components.contains(&ComponentKey::from("xform")),
            "Transform should be on authoritative path"
        );
        assert!(
            components.contains(&ComponentKey::from("auth_sink")),
            "Authoritative sink should be on authoritative path"
        );
        assert!(
            !components.contains(&ComponentKey::from("non_auth_sink")),
            "Non-authoritative sink should NOT be on authoritative path"
        );
    }

    #[test]
    fn fan_out_with_dedicated_non_auth_branch() {
        // This tests that components exclusively feeding non-authoritative sinks
        // are NOT on the authoritative path.
        //
        //   source1 -> auth_sink
        //   source2 -> xform_nonauth -> non_auth_sink
        //
        // Only source1 and auth_sink should be authoritative.
        let mut config = ConfigBuilder::default();
        config.add_source("source1", basic_source().1);
        config.add_source("source2", basic_source().1);
        config.add_transform("xform_nonauth", &["source2"], basic_transform("", 0.0));
        config.add_sink(
            "auth_sink",
            &["source1"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );
        config.add_sink(
            "non_auth_sink",
            &["xform_nonauth"],
            basic_sink_with_acks(10, ack_config(true, false)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some");

        assert!(
            components.contains(&ComponentKey::from("source1")),
            "source1 feeds auth_sink, should be authoritative"
        );
        assert!(
            components.contains(&ComponentKey::from("auth_sink")),
            "auth_sink is authoritative"
        );
        assert!(
            !components.contains(&ComponentKey::from("source2")),
            "source2 only feeds non-auth path, should NOT be authoritative"
        );
        assert!(
            !components.contains(&ComponentKey::from("xform_nonauth")),
            "xform_nonauth only feeds non-auth path, should NOT be authoritative"
        );
        assert!(
            !components.contains(&ComponentKey::from("non_auth_sink")),
            "non_auth_sink should NOT be on authoritative path"
        );
    }

    #[test]
    fn multiple_authoritative_sinks() {
        // source -> xform1 -> auth_sink1
        // source -> xform2 -> auth_sink2
        //
        // All components should be on the authoritative path.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_transform("xform1", &["src"], basic_transform(" a", 0.0));
        config.add_transform("xform2", &["src"], basic_transform(" b", 0.0));
        config.add_sink(
            "auth_sink1",
            &["xform1"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );
        config.add_sink(
            "auth_sink2",
            &["xform2"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some");

        let expected: HashSet<ComponentKey> = [
            ComponentKey::from("src"),
            ComponentKey::from("xform1"),
            ComponentKey::from("xform2"),
            ComponentKey::from("auth_sink1"),
            ComponentKey::from("auth_sink2"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            components, expected,
            "All paths to all authoritative sinks should be in the set"
        );
    }

    #[test]
    fn backwards_compat_no_authoritative_sinks_disables_stripping() {
        // Verify that when NO sink has authoritative: true, compute_authoritative_components
        // returns None, meaning the stripping mechanism is disabled and all sinks participate
        // in acks as before (backwards compatibility).
        let mut config = ConfigBuilder::default();
        config.add_source("src1", basic_source().1);
        config.add_source("src2", basic_source().1);
        config.add_transform("xform", &["src1"], basic_transform("", 0.0));
        // Sink with acks enabled but NOT authoritative (default behavior)
        config.add_sink(
            "sink1",
            &["xform"],
            basic_sink_with_acks(10, AcknowledgementsConfig::from(true)).1,
        );
        // Sink with default acks
        config.add_sink("sink2", &["src2"], basic_sink(10).1);

        let config = config.build().unwrap();
        assert!(
            config.compute_authoritative_components().is_none(),
            "When no sink has authoritative: true, the function should return None \
             to preserve backwards compatibility (all sinks participate in acks)"
        );
    }

    #[test]
    fn chain_of_transforms_all_on_authoritative_path() {
        // src -> t1 -> t2 -> t3 -> auth_sink
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_transform("t1", &["src"], basic_transform(" a", 0.0));
        config.add_transform("t2", &["t1"], basic_transform(" b", 0.0));
        config.add_transform("t3", &["t2"], basic_transform(" c", 0.0));
        config.add_sink(
            "auth_sink",
            &["t3"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some");

        let expected: HashSet<ComponentKey> = [
            ComponentKey::from("src"),
            ComponentKey::from("t1"),
            ComponentKey::from("t2"),
            ComponentKey::from("t3"),
            ComponentKey::from("auth_sink"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            components, expected,
            "Entire chain from source through transforms to auth_sink should be authoritative"
        );
    }

    #[test]
    fn authoritative_without_enabled_returns_none() {
        // A sink with authoritative: true but enabled: false should NOT activate the
        // authoritative stripping feature. This is a misconfiguration: marking a sink
        // as authoritative while opting out of e2e acks would strip finalizers from
        // other ack-enabled sinks without this sink actually participating in acks.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_sink(
            "auth_no_acks",
            &["src"],
            basic_sink_with_acks(10, ack_config(false, true)).1,
        );
        // A second sink with acks enabled but NOT authoritative (normal path)
        config.add_sink(
            "normal_sink",
            &["src"],
            basic_sink_with_acks(10, AcknowledgementsConfig::from(true)).1,
        );

        let config = config.build().unwrap();
        assert!(
            config.compute_authoritative_components().is_none(),
            "A sink with authoritative: true but enabled: false should not activate \
             the authoritative stripping feature; compute_authoritative_components \
             should return None"
        );
    }

    #[test]
    fn authoritative_with_enabled_false_excluded_from_set() {
        // When one sink has both authoritative + enabled, and another has authoritative
        // but NOT enabled, only the first should appear in the authoritative set.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_sink(
            "auth_enabled",
            &["src"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );
        config.add_sink(
            "auth_disabled",
            &["src"],
            basic_sink_with_acks(10, ack_config(false, true)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some because auth_enabled qualifies");

        assert!(
            components.contains(&ComponentKey::from("auth_enabled")),
            "Sink with both authoritative and enabled should be in the set"
        );
        assert!(
            !components.contains(&ComponentKey::from("auth_disabled")),
            "Sink with authoritative but NOT enabled should NOT be in the set"
        );
    }

    #[test]
    fn diamond_topology() {
        // Tests a diamond: source -> [t1, t2] -> merge_transform -> auth_sink
        // All components should be on the authoritative path.
        let mut config = ConfigBuilder::default();
        config.add_source("src", basic_source().1);
        config.add_transform("t1", &["src"], basic_transform(" left", 0.0));
        config.add_transform("t2", &["src"], basic_transform(" right", 0.0));
        config.add_sink(
            "auth_sink",
            &["t1", "t2"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some");

        let expected: HashSet<ComponentKey> = [
            ComponentKey::from("src"),
            ComponentKey::from("t1"),
            ComponentKey::from("t2"),
            ComponentKey::from("auth_sink"),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            components, expected,
            "Diamond topology: all components feeding auth_sink should be authoritative"
        );
    }

    #[test]
    fn separate_pipelines_not_in_authoritative_set() {
        // Two independent pipelines: one with authoritative, one without.
        // The non-authoritative pipeline components should NOT be in the authoritative
        // set. Legacy behavior is preserved at runtime by per-edge stripping: the
        // strip decision checks both upstream and downstream, and only strips when the
        // upstream IS in the set and the downstream is NOT. Since source_legacy is NOT
        // in the set, its edges to legacy_sink will never be stripped.
        let mut config = ConfigBuilder::default();
        config.add_source("src_auth", basic_source().1);
        config.add_source("src_legacy", basic_source().1);
        config.add_sink(
            "auth_sink",
            &["src_auth"],
            basic_sink_with_acks(10, ack_config(true, true)).1,
        );
        config.add_sink(
            "legacy_sink",
            &["src_legacy"],
            basic_sink_with_acks(10, AcknowledgementsConfig::from(true)).1,
        );

        let config = config.build().unwrap();
        let components = config
            .compute_authoritative_components()
            .expect("Should return Some because auth_sink qualifies");

        // The authoritative pipeline components should be in the set
        assert!(components.contains(&ComponentKey::from("src_auth")));
        assert!(components.contains(&ComponentKey::from("auth_sink")));

        // The legacy pipeline components should NOT be in the set
        assert!(
            !components.contains(&ComponentKey::from("src_legacy")),
            "Source with no authoritative downstream should NOT be in the authoritative set"
        );
        assert!(
            !components.contains(&ComponentKey::from("legacy_sink")),
            "Sink on non-authoritative source path should NOT be in the authoritative set"
        );
    }
}

use crate::{
    components::validation::{
        sync::{Configuring, TaskCoordinator},
        util::GrpcAddress,
        ComponentConfiguration, ComponentType, ValidationConfiguration,
    },
    config::ConfigBuilder,
    sinks::{vector::VectorConfig as VectorSinkConfig, Sinks},
    sources::{vector::VectorConfig as VectorSourceConfig, Sources},
    test_util::next_addr,
    transforms::Transforms,
};

use super::{
    io::{ControlledEdges, InputEdge, OutputEdge},
    telemetry::{Telemetry, TelemetryCollector},
};

pub struct TopologyBuilder {
    config_builder: ConfigBuilder,
    input_edge: Option<InputEdge>,
    output_edge: Option<OutputEdge>,
}

impl TopologyBuilder {
    /// Creates a component topology for the given component configuration.
    pub fn from_configuration(configuration: &ValidationConfiguration) -> Self {
        let component_configuration = configuration.component_configuration();
        match component_configuration {
            ComponentConfiguration::Source(source) => {
                debug_assert_eq!(configuration.component_type(), ComponentType::Source);
                Self::from_source(source)
            }
            ComponentConfiguration::Transform(transform) => {
                debug_assert_eq!(configuration.component_type(), ComponentType::Transform);
                Self::from_transform(transform)
            }
            ComponentConfiguration::Sink(sink) => {
                debug_assert_eq!(configuration.component_type(), ComponentType::Sink);
                Self::from_sink(sink)
            }
        }
    }

    /// Creates a component topology for validating a source.
    fn from_source(source: Sources) -> Self {
        let (output_edge, output_sink) = build_output_edge();

        let mut config_builder = ConfigBuilder::default();
        config_builder.add_source("test_source", source);
        config_builder.add_sink("output_sink", &["test_source"], output_sink);

        Self {
            config_builder,
            input_edge: None,
            output_edge: Some(output_edge),
        }
    }

    fn from_transform(transform: Transforms) -> Self {
        let (input_edge, input_source) = build_input_edge();
        let (output_edge, output_sink) = build_output_edge();

        let mut config_builder = ConfigBuilder::default();
        config_builder.add_source("input_source", input_source);
        config_builder.add_transform("test_transform", &["input_source"], transform);
        config_builder.add_sink("output_sink", &["test_transform"], output_sink);

        Self {
            config_builder,
            input_edge: Some(input_edge),
            output_edge: Some(output_edge),
        }
    }

    fn from_sink(sink: Sinks) -> Self {
        let (input_edge, input_source) = build_input_edge();

        let mut config_builder = ConfigBuilder::default();
        config_builder.add_source("input_source", input_source);
        config_builder.add_sink("test_sink", &["input_source"], sink);

        Self {
            config_builder,
            input_edge: Some(input_edge),
            output_edge: None,
        }
    }

    /// Finalizes the builder.
    ///
    /// The finalized configuration builder is returned, which can be used to create the running
    /// topology itself. All controlled edges are built and spawned, and a channel sender/receiver
    /// is provided for them. Additionally, the telemetry collector is also spawned and a channel
    /// receiver for telemetry events is provided.
    pub fn finalize(
        mut self,
        input_task_coordinator: &TaskCoordinator<Configuring>,
        output_task_coordinator: &TaskCoordinator<Configuring>,
    ) -> (ConfigBuilder, ControlledEdges, TelemetryCollector) {
        let controlled_edges = ControlledEdges {
            input: self
                .input_edge
                .map(|edge| edge.spawn_input_client(input_task_coordinator)),
            output: self
                .output_edge
                .map(|edge| edge.spawn_output_server(output_task_coordinator)),
        };

        let telemetry = Telemetry::attach_to_config(&mut self.config_builder);
        let telemetry_collector = telemetry.into_collector(output_task_coordinator);

        (self.config_builder, controlled_edges, telemetry_collector)
    }
}

fn build_input_edge() -> (InputEdge, impl Into<Sources>) {
    let input_listen_addr = GrpcAddress::from(next_addr());
    debug!(listen_addr = %input_listen_addr, "Creating controlled input edge.");

    let input_source = VectorSourceConfig::from_address(input_listen_addr.as_socket_addr());
    let input_edge = InputEdge::from_address(input_listen_addr);

    (input_edge, input_source)
}

fn build_output_edge() -> (OutputEdge, impl Into<Sinks>) {
    let output_listen_addr = GrpcAddress::from(next_addr());
    debug!(endpoint = %output_listen_addr, "Creating controlled output edge.");

    let output_sink = VectorSinkConfig::from_address(output_listen_addr.as_uri());
    let output_edge = OutputEdge::from_address(output_listen_addr);

    (output_edge, output_sink)
}

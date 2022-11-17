use http::Uri;

use crate::{
    config::ConfigBuilder,
    sinks::vector::VectorConfig as VectorSinkConfig,
    sources::{
        internal_logs::InternalLogsConfig, internal_metrics::InternalMetricsConfig, Sources,
    }, test_util::next_http_addr,
};

use super::edges::ControlledEdge;

const INTERNAL_LOGS_KEY: &str = "_telemetry_logs";
const INTERNAL_METRICS_KEY: &str = "_telemetry_metrics";
const VECTOR_SINK_KEY: &str = "_telemetry_out";

pub struct TopologyBuilder {
    config_builder: ConfigBuilder,
    controlled_edge: ControlledEdge,
    telemetry_listen_addr: Uri,
}

impl TopologyBuilder {
    pub fn from_source(source: Sources) -> Self {
        let output_listen_addr = next_http_addr();
        let telemetry_listen_addr = next_http_addr();

        // Our "controlled" edge is the sink that we'll attach to the source being validated in
        // order to form a complete topology and shuttle out the input events.
        let controlled_edge = ControlledEdge::output(output_listen_addr.clone());
        let output_sink = VectorSinkConfig::from_address(output_listen_addr);

        let mut config_builder = ConfigBuilder::default();
        config_builder.add_source("test_input", source);
        config_builder.add_sink("output_sink", &["test_input"], output_sink);

        attach_internal_telemetry_components(&mut config_builder, telemetry_listen_addr.clone());

        Self {
            config_builder,
            controlled_edge,
            telemetry_listen_addr,
        }
    }
}

fn attach_internal_telemetry_components(
    config_builder: &mut ConfigBuilder,
    telemetry_listen_addr: Uri,
) {
    // Attach an internal logs and internal metrics source, and send them on to a dedicated Vector
    // sink that we'll spawn a listener for to collect everything.
    let internal_logs = InternalLogsConfig::default();
    let internal_metrics = InternalMetricsConfig::default();
    let vector_sink = VectorSinkConfig::from_address(telemetry_listen_addr);

    config_builder.add_source(INTERNAL_LOGS_KEY, internal_logs);
    config_builder.add_source(INTERNAL_METRICS_KEY, internal_metrics);
    config_builder.add_sink(
        VECTOR_SINK_KEY,
        &[INTERNAL_LOGS_KEY, INTERNAL_METRICS_KEY],
        vector_sink,
    );
}

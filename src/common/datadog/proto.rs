//! Protocol Buffers types for various Datadog Agent payloads.

mod include {
    include!(concat!(env!("OUT_DIR"), "/dd-agent-protos/mod.rs"));
}

/// Metrics-specific Protocol Buffers types.
pub mod metrics {
    pub use super::include::dd_metric::{
        metric_payload::{MetricPoint, MetricSeries, MetricType, Resource},
        sketch_payload::{sketch::Dogsketch, Sketch},
        CommonMetadata, Metadata, MetricPayload, Origin, SketchPayload,
    };
    pub use super::include::ddsketch_full::*;
}

/// Traces-specific Protocol Buffers types.
pub mod traces {
    pub use super::include::dd_trace::*;
}

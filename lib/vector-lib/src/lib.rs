pub use vector_common::{
    assert_event_data_eq, btreemap, byte_size_of, byte_size_of::ByteSizeOf, conversion,
    encode_logfmt, finalization, finalizer, impl_event_data_eq, internal_event, json_size,
    registered_event, request_metadata, sensitive_string, shutdown, trigger, Error, Result,
    TimeZone,
};
pub use vector_config as configurable;
#[cfg(feature = "vrl")]
pub use vector_core::compile_vrl;
#[cfg(feature = "test")]
pub use vector_core::event_test_util;
pub use vector_core::{
    buckets, buffers, default_data_dir, event, fanout, metric_tags, metrics, partition, quantiles,
    samples, schema, serde, sink, source, tcp, tls, transform, update_counter,
    EstimatedJsonEncodedSizeOf,
};
pub use vector_stream as stream;

pub mod config {
    pub use vector_common::config::ComponentKey;
    pub use vector_core::config::{
        clone_input_definitions, init_log_schema, init_telemetry, log_schema, proxy, telemetry,
        AcknowledgementsConfig, DataType, GlobalOptions, Input, LegacyKey, LogNamespace, LogSchema,
        OutputId, SourceAcknowledgementsConfig, SourceOutput, Tags, Telemetry, TransformOutput,
        MEMORY_BUFFER_DEFAULT_MAX_EVENTS,
    };
}

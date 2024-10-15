pub use codecs;
pub use enrichment;
#[cfg(feature = "file-source")]
pub use file_source;
#[cfg(feature = "api-client")]
pub use vector_api_client as api_client;
pub use vector_buffers as buffers;
#[cfg(feature = "test")]
pub use vector_common::event_test_util;
pub use vector_common::{
    assert_event_data_eq, btreemap, byte_size_of, byte_size_of::ByteSizeOf, conversion,
    encode_logfmt, finalization, finalizer, id, impl_event_data_eq, internal_event, json_size,
    registered_event, request_metadata, sensitive_string, shutdown, trigger, Error, Result,
    TimeZone,
};
pub use vector_config as configurable;
pub use vector_config::impl_generate_config_from_default;
#[cfg(feature = "vrl")]
pub use vector_core::compile_vrl;
pub use vector_core::{
    buckets, default_data_dir, emit, event, fanout, ipallowlist, metric_tags, metrics, partition,
    quantiles, register, samples, schema, serde, sink, source, tcp, tls, transform,
    EstimatedJsonEncodedSizeOf,
};
pub use vector_lookup as lookup;
pub use vector_stream as stream;
pub use vector_tap as tap;
pub use vrl_cache;

pub mod config {
    pub use vector_common::config::ComponentKey;
    pub use vector_core::config::{
        clone_input_definitions, init_log_schema, init_telemetry, log_schema, proxy, telemetry,
        AcknowledgementsConfig, DataType, GlobalOptions, Input, LegacyKey, LogNamespace, LogSchema,
        OutputId, SourceAcknowledgementsConfig, SourceOutput, Tags, Telemetry, TransformOutput,
        MEMORY_BUFFER_DEFAULT_MAX_EVENTS,
    };
}

#[cfg(feature = "opentelemetry")]
pub mod opentelemetry {
    pub use opentelemetry_proto::{convert, proto};
}

#[cfg(feature = "prometheus")]
pub mod prometheus {
    pub use prometheus_parser as parser;
}

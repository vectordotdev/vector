//! Typed OTLP trace mapping (RFC 25329).
//!
//! Converts OTLP `ResourceSpans` into [`vector_core::event::typed_trace::TraceEvent`], and
//! encodes typed events back to `ExportTraceServiceRequest`. This module does not change
//! `Event::Trace` or the generic OTLP serializer.
//!
//! Stage 1 legacy layouts are not mapped to typed events directly. The legacy converter
//! parses each layout back into protobuf `ResourceSpans` and runs them through the same
//! decoding, so converting a legacy record and decoding wire input share one mapping.
//!
//! Gated behind the `typed-trace` cargo feature until a source, transform, or sink
//! consumes it.

mod any_value;
mod convert;
mod encode;
mod keys;
mod legacy;
mod timestamps;

pub use keys::{
    DATADOG_AGENT, DATADOG_CHUNK_DROPPED, DATADOG_CHUNK_ORIGIN, DATADOG_CHUNK_PRIORITY,
    DATADOG_CHUNK_TAGS, DATADOG_SPAN_META_STRUCT, DATADOG_SPAN_RESOURCE, DATADOG_SPAN_TYPE,
    DATADOG_TRACER_TAGS, ENVIRONMENT_LEGACY, ENVIRONMENT_NAME, HOST_NAME, SERVICE_NAME,
};
pub use legacy::{
    ConversionError, FailedConversion, HintedLegacy, hinted_legacy_to_typed,
    is_historical_otlp_layout, legacy_to_typed,
};

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

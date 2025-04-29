use std::io::Error;

// NOTE: Serde related attributes are copied opentelemetry-rust
// Source:  https://github.com/open-telemetry/opentelemetry-rust/
// File: opentelemetry-proto/tests/grpc_build.rs
// License: Apache-2.0
fn set_serde_attributes(mut builder: tonic_build::Builder) -> tonic_build::Builder {
    // Optional numeric, string and array fields need to default to their default value otherwise
    // JSON files without those field cannot deserialize
    // we cannot add serde(default) to all generated types because enums cannot be annotated with serde(default)
    for path in [
        "trace.v1.Span",
        "trace.v1.Span.Link",
        "trace.v1.ScopeSpans",
        "trace.v1.ResourceSpans",
        "common.v1.InstrumentationScope",
        "resource.v1.Resource",
        "trace.v1.Span.Event",
        "trace.v1.Status",
        "logs.v1.LogRecord",
        "logs.v1.ScopeLogs",
        "logs.v1.ResourceLogs",
        "metrics.v1.Metric",
        "metrics.v1.ResourceMetrics",
        "metrics.v1.ScopeMetrics",
        "metrics.v1.Gauge",
        "metrics.v1.Sum",
        "metrics.v1.Histogram",
        "metrics.v1.ExponentialHistogram",
        "metrics.v1.Summary",
        "metrics.v1.NumberDataPoint",
        "metrics.v1.HistogramDataPoint",
    ] {
        builder = builder.type_attribute(
            path,
            "#[cfg_attr(feature = \"with-serde\", serde(default))]",
        )
    }

    // special serializer and deserializer for traceId and spanId
    // OTLP/JSON format uses hex string for traceId and spanId
    // the proto file uses bytes for traceId and spanId
    // Thus, special serializer and deserializer are needed
    for path in [
        "trace.v1.Span.trace_id",
        "trace.v1.Span.span_id",
        "trace.v1.Span.parent_span_id",
        "trace.v1.Span.Link.trace_id",
        "trace.v1.Span.Link.span_id",
        "logs.v1.LogRecord.span_id",
        "logs.v1.LogRecord.trace_id",
        "metrics.v1.Exemplar.span_id",
        "metrics.v1.Exemplar.trace_id",
    ] {
        builder = builder
            .field_attribute(path, "#[cfg_attr(feature = \"with-serde\", serde(serialize_with = \"crate::proto::serializers::serialize_to_hex_string\", deserialize_with = \"crate::proto::serializers::deserialize_from_hex_string\"))]")
    }

    // flatten
    for path in [
        "metrics.v1.Metric.data",
        "metrics.v1.NumberDataPoint.value",
        "common.v1.AnyValue.value",
    ] {
        builder =
            builder.field_attribute(path, "#[cfg_attr(feature =\"with-serde\", serde(flatten))]");
    }
    builder
}

fn main() -> Result<(), Error> {
    let mut builder = tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .type_attribute(
            ".",
            "#[cfg_attr(feature = \"with-serde\", derive(serde::Serialize, serde::Deserialize))]",
        )
        .type_attribute(
            ".",
            "#[cfg_attr(feature = \"with-serde\", serde(rename_all = \"camelCase\"))]",
        );

    builder = set_serde_attributes(builder);

    // Note that according to Protobuf specs 64-bit integer numbers
    // in JSON-encoded payloads are encoded as decimal strings, and
    // either numbers or strings are accepted when decoding.
    for path in [
        "trace.v1.Span.start_time_unix_nano",
        "trace.v1.Span.end_time_unix_nano",
        "trace.v1.Span.Event.time_unix_nano",
        "logs.v1.LogRecord.time_unix_nano",
        "logs.v1.LogRecord.observed_time_unix_nano",
        "metrics.v1.HistogramDataPoint.start_time_unix_nano",
        "metrics.v1.HistogramDataPoint.time_unix_nano",
        "metrics.v1.NumberDataPoint.start_time_unix_nano",
        "metrics.v1.NumberDataPoint.time_unix_nano",
    ] {
        builder = builder
            .field_attribute(path, "#[cfg_attr(feature = \"with-serde\", serde(serialize_with = \"crate::proto::serializers::serialize_u64_to_string\", deserialize_with = \"crate::proto::serializers::deserialize_from_str_or_u64_to_u64\"))]")
    }

    builder
        .compile(
            &[
                "src/proto/opentelemetry-proto/opentelemetry/proto/common/v1/common.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/resource/v1/resource.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/logs/v1/logs.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/metrics/v1/metrics.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/trace/v1/trace.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/collector/trace/v1/trace_service.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/collector/logs/v1/logs_service.proto",
                "src/proto/opentelemetry-proto/opentelemetry/proto/collector/metrics/v1/metrics_service.proto",
            ],
            &["src/proto/opentelemetry-proto"],
        )?;

    Ok(())
}

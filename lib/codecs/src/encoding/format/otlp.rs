use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::{
    logs::native_log_to_otlp_request,
    proto::{
        DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
        RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
        TRACES_REQUEST_MESSAGE_TYPE,
    },
    spans::native_trace_to_otlp_request,
};
use prost::Message;
use tokio_util::codec::Encoder;
use vector_config_macros::configurable_component;
use vector_core::{config::DataType, event::Event, schema};
use vrl::protobuf::encode::Options;

/// Config used to build an `OtlpSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct OtlpSerializerConfig {
    // No configuration options needed - OTLP serialization is opinionated
}

impl OtlpSerializerConfig {
    /// Build the `OtlpSerializer` from this configuration.
    pub fn build(&self) -> Result<OtlpSerializer, crate::encoding::BuildError> {
        OtlpSerializer::new()
    }

    /// The data type of events that are accepted by `OtlpSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the OTLP (OpenTelemetry Protocol) protobuf format.
///
/// This serializer encodes events using the OTLP protobuf specification, which is the recommended
/// encoding format for OpenTelemetry data. The output is suitable for sending to OTLP-compatible
/// endpoints with `content-type: application/x-protobuf`.
///
/// # Implementation approach
///
/// This serializer converts Vector's internal event representation to the appropriate OTLP message type
/// based on the top-level field in the event:
/// - `resourceLogs` → `ExportLogsServiceRequest` (pre-formatted OTLP passthrough)
/// - `resourceMetrics` → `ExportMetricsServiceRequest` (pre-formatted OTLP passthrough)
/// - `resourceSpans` → `ExportTraceServiceRequest` (pre-formatted OTLP passthrough)
/// - Native logs (without `resourceLogs`) → Automatic conversion to `ExportLogsServiceRequest`
/// - Native traces (without `resourceSpans`) → Automatic conversion to `ExportTraceServiceRequest`
///
/// The implementation is the inverse of what the `opentelemetry` source does when decoding,
/// ensuring round-trip compatibility.
///
/// Native metric conversion (Counter, Gauge, Histogram, Summary, Distribution, Set) is
/// provided by companion PR #24897.
///
/// # Native Log Conversion
///
/// When a log event does not contain pre-formatted OTLP structure (`resourceLogs`), it is
/// automatically converted to OTLP format. This supports events from any source:
/// - OTLP receiver with `use_otlp_decoding: false` (flat decoded OTLP)
/// - File source with JSON/syslog logs
/// - Any other Vector source (socket, kafka, etc.)
///
/// Field mapping for native logs:
/// - `.message` / `.body` / `.msg` / `.log` → `logRecords[].body`
/// - `.timestamp` → `logRecords[].timeUnixNano`
/// - `.observed_timestamp` → `logRecords[].observedTimeUnixNano`
/// - `.attributes.*` → `logRecords[].attributes[]`
/// - `.resources.*` → `resource.attributes[]`
/// - `.severity_text` → `logRecords[].severityText`
/// - `.severity_number` → `logRecords[].severityNumber` (inferred from text if absent)
/// - `.scope.name/version/attributes` → `scopeLogs[].scope`
/// - `.trace_id` → `logRecords[].traceId` (hex string → bytes)
/// - `.span_id` → `logRecords[].spanId` (hex string → bytes)
/// - `.flags` → `logRecords[].flags`
/// - `.dropped_attributes_count` → `logRecords[].droppedAttributesCount`
/// - **All other fields** → `logRecords[].attributes[]` (automatic collection)
///
/// # Remaining Fields as Attributes
///
/// Any event field that is not a recognized OTLP field is automatically collected
/// into the `attributes[]` array to prevent data loss. For example, given a log event:
///
/// ```json
/// {"message": "User logged in", "level": "info", "user_id": "12345", "request_id": "abc-123"}
/// ```
///
/// The `message` maps to `body`, while `level`, `user_id`, and `request_id` are automatically
/// added to `attributes[]` with their original types preserved (string, integer, float, boolean,
/// array, and nested object values are all supported).
///
/// This behavior ensures that logs from any Vector source (file, syslog, socket, kafka, etc.)
/// can be sent to OTLP endpoints without manual field mapping. Fields already in `.attributes`
/// are combined with remaining fields in the output.
///
/// Vector operational metadata (`source_type`, `ingest_timestamp`) is excluded from this
/// automatic collection.
///
/// # Native Trace Conversion
///
/// When a trace event does not contain pre-formatted OTLP structure (`resourceSpans`), it is
/// automatically converted to OTLP format. Field mapping mirrors the decode path in `spans.rs`:
/// - `.trace_id` → `traceId` (hex string → 16 bytes)
/// - `.span_id` → `spanId` (hex string → 8 bytes)
/// - `.parent_span_id` → `parentSpanId` (hex string → 8 bytes)
/// - `.name` → `name`
/// - `.kind` → `kind`
/// - `.start_time_unix_nano` / `.end_time_unix_nano` → timestamps (nanos)
/// - `.attributes.*` → `attributes[]`
/// - `.resources.*` → `resource.attributes[]`
/// - `.events` → `events[]` (span events with name, time, attributes)
/// - `.links` → `links[]` (span links with trace_id, span_id, attributes)
/// - `.status` → `status` (message, code)
/// - **All other fields** → `attributes[]` (automatic collection, same as logs)
#[derive(Debug, Clone)]
pub struct OtlpSerializer {
    logs_descriptor: ProtobufSerializer,
    metrics_descriptor: ProtobufSerializer,
    traces_descriptor: ProtobufSerializer,
}

impl OtlpSerializer {
    /// Creates a new OTLP serializer with the appropriate message descriptors.
    pub fn new() -> vector_common::Result<Self> {
        let options = Options {
            use_json_names: true,
        };

        let logs_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            LOGS_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        let metrics_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            METRICS_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        let traces_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            TRACES_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        Ok(Self {
            logs_descriptor,
            metrics_descriptor,
            traces_descriptor,
        })
    }
}

impl Encoder<Event> for OtlpSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        // Determine which descriptor to use based on top-level OTLP fields
        // This handles events that were decoded with use_otlp_decoding enabled
        // The deserializer uses use_json_names: true, so fields are in camelCase
        match &event {
            Event::Log(log) => {
                if log.contains(RESOURCE_LOGS_JSON_FIELD) {
                    // Pre-formatted OTLP logs - encode directly (existing behavior)
                    self.logs_descriptor.encode(event, buffer)
                } else if log.contains(RESOURCE_METRICS_JSON_FIELD) {
                    // Pre-formatted OTLP metrics (as Vector logs) - encode directly
                    self.metrics_descriptor.encode(event, buffer)
                } else {
                    // Native Vector format - convert to OTLP
                    // This handles events from any source (file, socket, otlp with
                    // use_otlp_decoding: false, etc.) with graceful degradation
                    // for invalid fields
                    let otlp_request = native_log_to_otlp_request(log);
                    otlp_request
                        .encode(buffer)
                        .map_err(|e| format!("Failed to encode OTLP request: {e}").into())
                }
            }
            Event::Trace(trace) => {
                if trace.contains(RESOURCE_SPANS_JSON_FIELD) {
                    self.traces_descriptor.encode(event, buffer)
                } else {
                    // Native Vector format - convert to OTLP
                    // This handles trace events from any source (otlp with
                    // use_otlp_decoding: false, datadog_agent, etc.) with
                    // graceful degradation for invalid fields
                    let otlp_request = native_trace_to_otlp_request(trace);
                    otlp_request
                        .encode(buffer)
                        .map_err(|e| format!("Failed to encode OTLP trace request: {e}").into())
                }
            }
            Event::Metric(_) => {
                // Native metric → OTLP conversion is provided by #24897.
                // Until that PR is merged, metrics require use_otlp_decoding: true
                // on the source for passthrough encoding.
                Err("OTLP serializer does not support native Vector metrics yet. \
                     Use `use_otlp_decoding: true` on the source for metrics passthrough, \
                     or see PR #24897 for native metric conversion."
                    .into())
            }
        }
    }
}

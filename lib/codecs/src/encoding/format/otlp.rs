use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::{
    logs::native_log_to_otlp_request,
    proto::{
        DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
        RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
        TRACES_REQUEST_MESSAGE_TYPE,
    },
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
        DataType::Log | DataType::Trace
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
///
/// The implementation is the inverse of what the `opentelemetry` source does when decoding,
/// ensuring round-trip compatibility.
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
/// - `.message` / `.body` / `.msg` → `logRecords[].body.stringValue`
/// - `.timestamp` → `logRecords[].timeUnixNano`
/// - `.attributes.*` → `logRecords[].attributes[]`
/// - `.resources.*` → `resource.attributes[]`
/// - `.severity_text` → `logRecords[].severityText`
/// - `.severity_number` → `logRecords[].severityNumber`
/// - `.scope.name/version` → `scopeLogs[].scope`
/// - `.trace_id` → `logRecords[].traceId` (hex string → bytes)
/// - `.span_id` → `logRecords[].spanId` (hex string → bytes)
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields will be used once encoding is implemented
pub struct OtlpSerializer {
    logs_descriptor: ProtobufSerializer,
    metrics_descriptor: ProtobufSerializer,
    traces_descriptor: ProtobufSerializer,
    options: Options,
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
            options,
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
                    Err(
                        "Trace event does not contain OTLP structure and native conversion is not yet supported".into(),
                    )
                }
            }
            Event::Metric(_) => {
                Err("OTLP serializer does not support native Vector metrics yet.".into())
            }
        }
    }
}

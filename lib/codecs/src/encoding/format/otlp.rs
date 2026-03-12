use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::{
    metrics::native_metric_to_otlp_request,
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
/// This serializer converts Vector's internal event representation to the appropriate OTLP message type:
/// - `resourceLogs` → `ExportLogsServiceRequest` (pre-formatted OTLP passthrough)
/// - `resourceMetrics` → `ExportMetricsServiceRequest` (pre-formatted OTLP passthrough)
/// - `resourceSpans` → `ExportTraceServiceRequest` (pre-formatted OTLP passthrough)
/// - Native metrics → Automatic conversion to `ExportMetricsServiceRequest`
///
/// The implementation is the inverse of what the `opentelemetry` source does when decoding,
/// ensuring round-trip compatibility.
///
/// # Native Metric Conversion
///
/// Native Vector metrics are automatically converted to OTLP format:
/// - Counter → Sum (monotonic, Delta/Cumulative based on MetricKind)
/// - Gauge → Gauge
/// - AggregatedHistogram → Histogram
/// - AggregatedSummary → Summary
/// - Distribution → Histogram (samples converted to buckets)
/// - Set → Gauge (cardinality count)
///
/// Tag decomposition (reverse of decode-path flattening):
/// - `resource.*` tags → `resource.attributes[]`
/// - `scope.name` / `scope.version` → `scopeMetrics[].scope`
/// - `scope.*` tags → scope attributes
/// - All other tags → data point `attributes[]`
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
                    self.logs_descriptor.encode(event, buffer)
                } else if log.contains(RESOURCE_METRICS_JSON_FIELD) {
                    // Currently the OTLP metrics are Vector logs (not metrics).
                    self.metrics_descriptor.encode(event, buffer)
                } else {
                    Err(format!(
                        "Log event does not contain OTLP top-level fields ({RESOURCE_LOGS_JSON_FIELD} or {RESOURCE_METRICS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Trace(trace) => {
                if trace.contains(RESOURCE_SPANS_JSON_FIELD) {
                    self.traces_descriptor.encode(event, buffer)
                } else {
                    Err(format!(
                        "Trace event does not contain OTLP top-level field ({RESOURCE_SPANS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Metric(metric) => {
                // Native Vector metric → OTLP conversion
                // Tags are decomposed back into resource/scope/data-point attributes
                let otlp_request = native_metric_to_otlp_request(metric);
                otlp_request
                    .encode(buffer)
                    .map_err(|e| format!("Failed to encode OTLP metric request: {e}").into())
            }
        }
    }
}

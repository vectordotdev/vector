use bytes::Bytes;
use opentelemetry_proto::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
    TRACES_REQUEST_MESSAGE_TYPE,
};
use smallvec::{SmallVec, smallvec};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace, log_schema},
    event::Event,
    schema,
};
use vrl::{protobuf::parse::Options, value::Kind};

use super::{Deserializer, ProtobufDeserializer};

/// Config used to build an `OtlpDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct OtlpDeserializerConfig {}

impl OtlpDeserializerConfig {
    /// Build the `OtlpDeserializer` from this configuration.
    pub fn build(&self) -> vector_common::Result<OtlpDeserializer> {
        OtlpDeserializer::new()
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log | DataType::Trace
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                schema::Definition::empty_legacy_namespace().unknown_fields(Kind::any())
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::any(), [log_namespace])
            }
        }
    }
}

/// Deserializer that builds `Event`s from a byte frame containing OTLP (OpenTelemetry Protocol) protobuf data.
///
/// This deserializer decodes events using the OTLP protobuf specification. It handles the three
/// OTLP signal types: logs, metrics, and traces.
///
/// # Implementation approach
///
/// This deserializer converts OTLP protobuf messages to Vector's internal event representation.
/// The implementation supports three OTLP message types:
/// - `ExportLogsServiceRequest` → Log events with `resourceLogs` field
/// - `ExportMetricsServiceRequest` → Log events with `resourceMetrics` field (metrics as logs)
/// - `ExportTraceServiceRequest` → Trace events with `resourceSpans` field
///
/// This is the inverse of what the OTLP encoder does, ensuring round-trip compatibility
/// with the `opentelemetry` source when `use_otlp_decoding` is enabled.
#[derive(Debug, Clone)]
pub struct OtlpDeserializer {
    logs_deserializer: ProtobufDeserializer,
    metrics_deserializer: ProtobufDeserializer,
    traces_deserializer: ProtobufDeserializer,
}

impl OtlpDeserializer {
    /// Creates a new OTLP deserializer with the appropriate protobuf deserializers.
    pub fn new() -> vector_common::Result<Self> {
        let options = Options {
            use_json_names: true,
        };

        let logs_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            LOGS_REQUEST_MESSAGE_TYPE,
            options.clone(),
        )?;

        let metrics_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            METRICS_REQUEST_MESSAGE_TYPE,
            options.clone(),
        )?;

        let traces_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            TRACES_REQUEST_MESSAGE_TYPE,
            options,
        )?;

        Ok(Self {
            logs_deserializer,
            metrics_deserializer,
            traces_deserializer,
        })
    }
}

impl Deserializer for OtlpDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // Try parsing as logs and check for resourceLogs field
        if let Ok(events) = self.logs_deserializer.parse(bytes.clone(), log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_LOGS_JSON_FIELD).is_some() {
                    return Ok(events);
                }

        // Try parsing as metrics and check for resourceMetrics field
        if let Ok(events) = self
            .metrics_deserializer
            .parse(bytes.clone(), log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_METRICS_JSON_FIELD).is_some() {
                    return Ok(events);
                }

        // Try parsing as traces and check for resourceSpans field
        if let Ok(mut events) = self.traces_deserializer.parse(bytes, log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_SPANS_JSON_FIELD).is_some() {
                    // Convert the log event to a trace event by taking ownership
                    if let Some(Event::Log(log)) = events.pop() {
                        let trace_event = Event::Trace(log.into());
                        return Ok(smallvec![trace_event]);
                    }
                }

        Err(format!(
            "Invalid OTLP data: expected '{RESOURCE_LOGS_JSON_FIELD}', '{RESOURCE_METRICS_JSON_FIELD}', or '{RESOURCE_SPANS_JSON_FIELD}'",
        )
            .into())
    }
}

#[cfg(test)]
mod tests {
    use opentelemetry_proto::proto::{
        collector::{
            logs::v1::ExportLogsServiceRequest, metrics::v1::ExportMetricsServiceRequest,
            trace::v1::ExportTraceServiceRequest,
        },
        logs::v1::{LogRecord, ResourceLogs, ScopeLogs},
        metrics::v1::{Metric, ResourceMetrics, ScopeMetrics},
        resource::v1::Resource,
        trace::v1::{ResourceSpans, ScopeSpans, Span},
    };
    use prost::Message;

    use super::*;

    fn create_logs_request_bytes() -> Bytes {
        let request = ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![LogRecord {
                        time_unix_nano: 1234567890,
                        severity_number: 9,
                        severity_text: "INFO".to_string(),
                        body: None,
                        attributes: vec![],
                        dropped_attributes_count: 0,
                        flags: 0,
                        trace_id: vec![],
                        span_id: vec![],
                        observed_time_unix_nano: 0,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        Bytes::from(request.encode_to_vec())
    }

    fn create_metrics_request_bytes() -> Bytes {
        let request = ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![Metric {
                        name: "test_metric".to_string(),
                        description: String::new(),
                        unit: String::new(),
                        data: None,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        Bytes::from(request.encode_to_vec())
    }

    fn create_traces_request_bytes() -> Bytes {
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![],
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        trace_id: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
                        span_id: vec![1, 2, 3, 4, 5, 6, 7, 8],
                        trace_state: String::new(),
                        parent_span_id: vec![],
                        name: "test_span".to_string(),
                        kind: 0,
                        start_time_unix_nano: 1234567890,
                        end_time_unix_nano: 1234567900,
                        attributes: vec![],
                        dropped_attributes_count: 0,
                        events: vec![],
                        dropped_events_count: 0,
                        links: vec![],
                        dropped_links_count: 0,
                        status: None,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        Bytes::from(request.encode_to_vec())
    }

    fn assert_otlp_event(bytes: Bytes, field: &str, is_trace: bool) {
        let deserializer = OtlpDeserializer::new().unwrap();
        let events = deserializer.parse(bytes, LogNamespace::Legacy).unwrap();

        assert_eq!(events.len(), 1);
        if is_trace {
            assert!(matches!(events[0], Event::Trace(_)));
            assert!(events[0].as_trace().get(field).is_some());
        } else {
            assert!(events[0].as_log().get(field).is_some());
        }
    }

    #[test]
    fn deserialize_otlp_logs() {
        assert_otlp_event(create_logs_request_bytes(), RESOURCE_LOGS_JSON_FIELD, false);
    }

    #[test]
    fn deserialize_otlp_metrics() {
        assert_otlp_event(
            create_metrics_request_bytes(),
            RESOURCE_METRICS_JSON_FIELD,
            false,
        );
    }

    #[test]
    fn deserialize_otlp_traces() {
        assert_otlp_event(
            create_traces_request_bytes(),
            RESOURCE_SPANS_JSON_FIELD,
            true,
        );
    }

    #[test]
    fn deserialize_invalid_otlp() {
        let deserializer = OtlpDeserializer::new().unwrap();
        let bytes = Bytes::from("invalid protobuf data");
        let result = deserializer.parse(bytes, LogNamespace::Legacy);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid OTLP data")
        );
    }
}

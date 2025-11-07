use bytes::Bytes;
use opentelemetry_proto::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
    TRACES_REQUEST_MESSAGE_TYPE,
};
use smallvec::{SmallVec, smallvec};
use vector_config::{configurable_component, indexmap::IndexSet};
use vector_core::{
    config::{DataType, LogNamespace},
    event::Event,
    schema,
};
use vrl::{protobuf::parse::Options, value::Kind};

use super::{Deserializer, ProtobufDeserializer};

/// OTLP signal type for prioritized parsing.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OtlpSignalType {
    /// OTLP logs signal (ExportLogsServiceRequest)
    Logs,
    /// OTLP metrics signal (ExportMetricsServiceRequest)
    Metrics,
    /// OTLP traces signal (ExportTraceServiceRequest)
    Traces,
}

/// Config used to build an `OtlpDeserializer`.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct OtlpDeserializerConfig {
    /// Signal types to attempt parsing, in priority order.
    ///
    /// The deserializer will try parsing in the order specified. This allows you to optimize
    /// performance when you know the expected signal types. For example, if you only receive
    /// traces, set this to `["traces"]` to avoid attempting to parse as logs or metrics first.
    ///
    /// If not specified, defaults to trying all types in order: logs, metrics, traces.
    /// Duplicate signal types are automatically removed while preserving order.
    #[serde(default = "default_signal_types")]
    pub signal_types: IndexSet<OtlpSignalType>,
}

fn default_signal_types() -> IndexSet<OtlpSignalType> {
    IndexSet::from([
        OtlpSignalType::Logs,
        OtlpSignalType::Metrics,
        OtlpSignalType::Traces,
    ])
}

impl Default for OtlpDeserializerConfig {
    fn default() -> Self {
        Self {
            signal_types: default_signal_types(),
        }
    }
}

impl OtlpDeserializerConfig {
    /// Build the `OtlpDeserializer` from this configuration.
    pub fn build(&self) -> OtlpDeserializer {
        OtlpDeserializer::new_with_signals(self.signal_types.clone())
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

/// Deserializer that builds `Event`s from a byte frame containing [OTLP](https://opentelemetry.io/docs/specs/otlp/) protobuf data.
///
/// This deserializer decodes events using the OTLP protobuf specification. It handles the three
/// OTLP signal types: logs, metrics, and traces.
///
/// The implementation supports three OTLP message types:
/// - `ExportLogsServiceRequest` → Log events with `resourceLogs` field
/// - `ExportMetricsServiceRequest` → Log events with `resourceMetrics` field
/// - `ExportTraceServiceRequest` → Trace events with `resourceSpans` field
///
/// One major caveat here is that the incoming metrics will be parsed as logs but they will preserve the OTLP format.
/// This means that components that work on metrics, will not be compatible with this output.
/// However, these events can be forwarded directly to a downstream OTEL collector.
///
/// This is the inverse of what the OTLP encoder does, ensuring round-trip compatibility
/// with the `opentelemetry` source when `use_otlp_decoding` is enabled.
#[derive(Debug, Clone)]
pub struct OtlpDeserializer {
    logs_deserializer: ProtobufDeserializer,
    metrics_deserializer: ProtobufDeserializer,
    traces_deserializer: ProtobufDeserializer,
    /// Signal types to parse, in priority order
    signals: IndexSet<OtlpSignalType>,
}

impl Default for OtlpDeserializer {
    fn default() -> Self {
        Self::new_with_signals(default_signal_types())
    }
}

impl OtlpDeserializer {
    /// Creates a new OTLP deserializer with custom signal support.
    /// During parsing, each signal type is tried in order until one succeeds.
    pub fn new_with_signals(signals: IndexSet<OtlpSignalType>) -> Self {
        let options = Options {
            use_json_names: true,
        };

        let logs_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            LOGS_REQUEST_MESSAGE_TYPE,
            options.clone(),
        )
        .expect("Failed to create logs deserializer");

        let metrics_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            METRICS_REQUEST_MESSAGE_TYPE,
            options.clone(),
        )
        .expect("Failed to create metrics deserializer");

        let traces_deserializer = ProtobufDeserializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            TRACES_REQUEST_MESSAGE_TYPE,
            options,
        )
        .expect("Failed to create traces deserializer");

        Self {
            logs_deserializer,
            metrics_deserializer,
            traces_deserializer,
            signals,
        }
    }
}

impl Deserializer for OtlpDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // Try parsing in the priority order specified
        for signal_type in &self.signals {
            match signal_type {
                OtlpSignalType::Logs => {
                    if let Ok(events) = self.logs_deserializer.parse(bytes.clone(), log_namespace)
                        && let Some(Event::Log(log)) = events.first()
                        && log.get(RESOURCE_LOGS_JSON_FIELD).is_some()
                    {
                        return Ok(events);
                    }
                }
                OtlpSignalType::Metrics => {
                    if let Ok(events) = self
                        .metrics_deserializer
                        .parse(bytes.clone(), log_namespace)
                        && let Some(Event::Log(log)) = events.first()
                        && log.get(RESOURCE_METRICS_JSON_FIELD).is_some()
                    {
                        return Ok(events);
                    }
                }
                OtlpSignalType::Traces => {
                    if let Ok(mut events) =
                        self.traces_deserializer.parse(bytes.clone(), log_namespace)
                        && let Some(Event::Log(log)) = events.first()
                        && log.get(RESOURCE_SPANS_JSON_FIELD).is_some()
                    {
                        // Convert the log event to a trace event by taking ownership
                        if let Some(Event::Log(log)) = events.pop() {
                            let trace_event = Event::Trace(log.into());
                            return Ok(smallvec![trace_event]);
                        }
                    }
                }
            }
        }

        Err(format!("Invalid OTLP data: expected one of {:?}", self.signals).into())
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

    // trace_id: 0102030405060708090a0b0c0d0e0f10 (16 bytes)
    const TEST_TRACE_ID: [u8; 16] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10,
    ];
    // span_id: 0102030405060708 (8 bytes)
    const TEST_SPAN_ID: [u8; 8] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

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
                        trace_id: TEST_TRACE_ID.to_vec(),
                        span_id: TEST_SPAN_ID.to_vec(),
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

    fn validate_trace_ids(trace: &vrl::value::Value) {
        // Navigate to the span and check traceId and spanId
        let resource_spans = trace
            .get("resourceSpans")
            .and_then(|v| v.as_array())
            .expect("resourceSpans should be an array");

        let first_rs = resource_spans
            .first()
            .expect("should have at least one resource span");

        let scope_spans = first_rs
            .get("scopeSpans")
            .and_then(|v| v.as_array())
            .expect("scopeSpans should be an array");

        let first_ss = scope_spans
            .first()
            .expect("should have at least one scope span");

        let spans = first_ss
            .get("spans")
            .and_then(|v| v.as_array())
            .expect("spans should be an array");

        let span = spans.first().expect("should have at least one span");

        // Verify traceId - should be raw bytes (16 bytes for trace_id)
        let trace_id = span
            .get("traceId")
            .and_then(|v| v.as_bytes())
            .expect("traceId should exist and be bytes");

        assert_eq!(
            trace_id.as_ref(),
            &TEST_TRACE_ID,
            "traceId should match the expected 16 bytes (0102030405060708090a0b0c0d0e0f10)"
        );

        // Verify spanId - should be raw bytes (8 bytes for span_id)
        let span_id = span
            .get("spanId")
            .and_then(|v| v.as_bytes())
            .expect("spanId should exist and be bytes");

        assert_eq!(
            span_id.as_ref(),
            &TEST_SPAN_ID,
            "spanId should match the expected 8 bytes (0102030405060708)"
        );
    }

    fn assert_otlp_event(bytes: Bytes, field: &str, is_trace: bool) {
        let deserializer = OtlpDeserializer::default();
        let events = deserializer.parse(bytes, LogNamespace::Legacy).unwrap();

        assert_eq!(events.len(), 1);
        if is_trace {
            assert!(matches!(events[0], Event::Trace(_)));
            let trace = events[0].as_trace();
            assert!(trace.get(field).is_some());
            validate_trace_ids(trace.value());
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
        let deserializer = OtlpDeserializer::default();
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

    #[test]
    fn deserialize_with_custom_priority_traces_only() {
        // Configure to only try traces - should succeed for traces, fail for others
        let deserializer =
            OtlpDeserializer::new_with_signals(IndexSet::from([OtlpSignalType::Traces]));

        // Traces should work
        let trace_bytes = create_traces_request_bytes();
        let result = deserializer.parse(trace_bytes, LogNamespace::Legacy);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap()[0], Event::Trace(_)));

        // Logs should fail since we're not trying to parse logs
        let log_bytes = create_logs_request_bytes();
        let result = deserializer.parse(log_bytes, LogNamespace::Legacy);
        assert!(result.is_err());
    }
}

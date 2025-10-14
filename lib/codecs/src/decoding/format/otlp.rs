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
pub struct OtlpDeserializerConfig {
    // No configuration options needed - OTLP deserialization is opinionated
}

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
                let mut definition =
                    schema::Definition::empty_legacy_namespace().unknown_fields(Kind::any());

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        // The OTLP decoder will try to insert a new `timestamp`-type value into the
                        // "timestamp_key" field, but only if that field doesn't already exist.
                        Kind::any().or_timestamp(),
                        Some("timestamp"),
                    );
                }
                definition
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

    /// Try to parse as logs and verify it has the expected top-level field.
    fn try_parse_logs(
        &self,
        bytes: &Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<Option<SmallVec<[Event; 1]>>> {
        if let Ok(events) = self.logs_deserializer.parse(bytes.clone(), log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_LOGS_JSON_FIELD).is_some()
        {
            return Ok(Some(events));
        }
        Ok(None)
    }

    /// Try to parse as metrics and verify it has the expected top-level field.
    /// Note: These are parsed as Log events (not Metric events) to preserve the OTLP structure.
    fn try_parse_metrics(
        &self,
        bytes: &Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<Option<SmallVec<[Event; 1]>>> {
        if let Ok(events) = self
            .metrics_deserializer
            .parse(bytes.clone(), log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_METRICS_JSON_FIELD).is_some()
        {
            return Ok(Some(events));
        }
        Ok(None)
    }

    /// Try to parse as traces and verify it has the expected top-level field.
    /// This creates TraceEvent instead of LogEvent.
    fn try_parse_traces(
        &self,
        bytes: &Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<Option<SmallVec<[Event; 1]>>> {
        // Parse as a log event first to get the VRL value
        if let Ok(mut events) = self.traces_deserializer.parse(bytes.clone(), log_namespace)
            && let Some(Event::Log(log)) = events.first()
            && log.get(RESOURCE_SPANS_JSON_FIELD).is_some()
        {
            // Convert the log event to a trace event by taking ownership
            if let Some(Event::Log(log)) = events.pop() {
                let trace_event = Event::Trace(log.into());
                return Ok(Some(smallvec![trace_event]));
            }
        }
        Ok(None)
    }
}

impl Deserializer for OtlpDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // Try parsing as logs first
        if let Some(events) = self.try_parse_logs(&bytes, log_namespace)? {
            return Ok(events);
        }

        // Try parsing as metrics
        if let Some(events) = self.try_parse_metrics(&bytes, log_namespace)? {
            return Ok(events);
        }

        // Try parsing as traces
        if let Some(events) = self.try_parse_traces(&bytes, log_namespace)? {
            return Ok(events);
        }

        Err("Failed to decode bytes as any OTLP message type (logs, metrics, or traces)".into())
    }
}

//! The Sentry [`vector_lib::sink::VectorSink`].
//!
//! This module contains the [`vector_lib::sink::VectorSink`] instance that is responsible for
//! taking a stream of [`vector_lib::event::Event`]s and forwarding them to Sentry.

use std::time::SystemTime;

use futures::FutureExt;
use sentry::protocol::{Log, LogAttribute, LogLevel, Map, TraceId, Value};
use tracing::warn;
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesSent, CountByteSize, EventsSent, InternalEventHandle, Output, Protocol,
};
use vector_lib::json_size::JsonSize;
use vector_lib::sensitive_string::SensitiveString;
use vrl::value::Kind;

use crate::{
    codecs::{EncodingConfigWithFraming, Transformer},
    sinks::prelude::*,
};

/// Configuration for the `sentry` sink.
#[configurable_component(sink("sentry", "Deliver log events to Sentry."))]
#[derive(Clone, Debug)]
pub struct SentryConfig {
    /// The Sentry DSN (Data Source Name) to send logs to.
    #[configurable(metadata(docs::examples = "${SENTRY_DSN}"))]
    #[configurable(metadata(docs::examples = "https://key@sentry.io/project_id"))]
    dsn: SensitiveString,

    /// Enable debug logging for the Sentry transport.
    ///
    /// When enabled, this will output debug information about the Sentry client
    /// including details about event transmission and transport operations.
    ///
    /// Defaults to `false`.
    #[configurable(metadata(docs::examples = true))]
    #[serde(default)]
    sentry_debug: bool,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<SentryDefaultBatchSettings>,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

#[derive(Clone, Copy, Debug, Default)]
struct SentryDefaultBatchSettings;

impl SinkBatchSettings for SentryDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(100);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

impl GenerateConfig for SentryConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(r#"dsn = "${SENTRY_DSN}""#).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sentry")]
impl SinkConfig for SentryConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let batch_settings = self.batch.validate()?.into_batcher_settings()?;

        let transformer = self.encoding.transformer();

        let sentry_sink: SentrySink = SentrySink::new(
            self.dsn.inner(),
            self.sentry_debug,
            transformer,
            batch_settings,
        );

        let boxed_healthcheck = healthcheck(self.dsn.clone()).boxed();

        Ok((
            VectorSink::from_event_streamsink(sentry_sink),
            boxed_healthcheck,
        ))
    }

    fn input(&self) -> Input {
        let requirement = Requirement::empty().optional_meaning("timestamp", Kind::timestamp());

        Input::log().with_schema_requirement(requirement)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Estimate the serialized byte size of a Sentry log for telemetry purposes.
/// This provides an approximation since we don't have access to the actual serialized bytes.
fn estimate_sentry_log_size(log: &Log) -> usize {
    let mut size = 0;

    // Base log structure overhead
    size += 100; // Rough estimate for JSON structure, timestamp, level, etc.

    // Body/message size
    size += log.body.len();

    // Trace ID (if present)
    if let Some(trace_id) = log.trace_id {
        size += trace_id.to_string().len();
    }

    // Attributes
    for (key, value) in &log.attributes {
        size += key.len();
        size += match &value.0 {
            sentry::protocol::Value::String(s) => s.len(),
            sentry::protocol::Value::Number(_) => 20, // Rough estimate for number
            sentry::protocol::Value::Bool(_) => 5,    // "true" or "false"
            sentry::protocol::Value::Array(arr) => arr.len() * 50, // Rough estimate per array element
            sentry::protocol::Value::Object(obj) => obj.len() * 50, // Rough estimate per object field
            _ => 20,                                                // Fallback for other types
        };
    }

    size
}

pub(super) struct SentrySink {
    transformer: Transformer,
    batch_settings: BatcherSettings,
    _sentry_guard: sentry::ClientInitGuard,
}

impl SentrySink {
    pub(super) fn new(
        dsn: &str,
        sentry_debug: bool,
        transformer: Transformer,
        batch_settings: BatcherSettings,
    ) -> Self {
        let sentry_guard = sentry::init((
            dsn,
            sentry::ClientOptions {
                enable_logs: true,
                debug: sentry_debug,
                server_name: None,
                before_send_log: Some(std::sync::Arc::new(|mut log: Log| {
                    log.attributes
                        .insert("sentry.sdk.name".into(), "sentry.vector.sink".into());
                    log.attributes
                        .insert("sentry.sdk.version".into(), "0.1.0".into());
                    Some(log)
                })),
                ..Default::default()
            },
        ));

        Self {
            transformer,
            batch_settings,
            _sentry_guard: sentry_guard,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        // Register telemetry events
        let bytes_sent = register!(BytesSent::from(Protocol("sentry".into())));
        let events_sent = register!(EventsSent::from(Output(None)));

        input
            .batched(self.batch_settings.as_byte_size_config())
            .for_each(|events| async {
                let transformer = self.transformer.clone();
                let mut batch_finalizers = Vec::new();
                let mut batch_count = 0;
                let mut batch_json_size = JsonSize::zero();
                let mut batch_successful = true;

                for mut event in events {
                    // Extract finalizers before processing
                    let finalizers = event.take_finalizers();
                    let event_byte_size = event.estimated_json_encoded_size_of();

                    transformer.transform(&mut event);

                    if let Event::Log(log) = event {
                        // Convert to Sentry log and send
                        let sentry_log = convert_to_sentry_log(&log);

                        // Estimate the serialized byte size of the Sentry log
                        // This is an approximation since we don't have access to the actual bytes sent
                        let estimated_sentry_bytes = estimate_sentry_log_size(&sentry_log);

                        // Try to send to Sentry
                        match std::panic::catch_unwind(|| {
                            sentry::Hub::main().capture_log(sentry_log)
                        }) {
                            Ok(_) => {
                                // Success - track metrics for this event
                                batch_count += 1;
                                batch_json_size += event_byte_size;
                                batch_finalizers.push((
                                    finalizers,
                                    EventStatus::Delivered,
                                    estimated_sentry_bytes,
                                ));
                            }
                            Err(_) => {
                                // Sentry client panicked or failed
                                warn!("Failed to send log to Sentry");
                                batch_finalizers.push((finalizers, EventStatus::Errored, 0));
                                batch_successful = false;
                            }
                        }
                    } else {
                        // Non-log events are not supported by Sentry
                        batch_finalizers.push((finalizers, EventStatus::Delivered, 0));
                    }
                }

                // Update finalizer status and emit telemetry events
                let mut total_sentry_bytes = 0;
                for (finalizers, status, sentry_bytes) in batch_finalizers {
                    finalizers.update_status(status);
                    if status == EventStatus::Delivered {
                        total_sentry_bytes += sentry_bytes;
                    }
                }

                // Emit telemetry events for successful batch
                if batch_successful && batch_count > 0 {
                    events_sent.emit(CountByteSize(batch_count, batch_json_size));
                    bytes_sent.emit(ByteSize(total_sentry_bytes));
                }
            })
            .await;

        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for SentrySink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Extract trace ID from log event, returning the trace ID and which field was used.
fn extract_trace_id(log: &vector_lib::event::LogEvent) -> (TraceId, Option<&'static str>) {
    let trace_fields = ["trace_id", "sentry.trace_id"];
    for field_name in &trace_fields {
        if let Some(trace_value) = log.get(*field_name) {
            let trace_str = trace_value.to_string_lossy();
            if let Ok(uuid) = uuid::Uuid::parse_str(&trace_str) {
                // Convert UUID to bytes and then to TraceId
                return (TraceId::from(uuid.into_bytes()), Some(*field_name));
            }
        }
    }

    // Create a zero'd out trace ID (16 bytes of zeros for UUID). This is special cased
    // during sentry ingestion.
    let default_trace_id: TraceId = TraceId::from([0u8; 16]);

    (default_trace_id, None)
}

/// Convert a Vector log event to a Sentry log.
fn convert_to_sentry_log(log: &vector_lib::event::LogEvent) -> Log {
    // Extract timestamp
    let timestamp = log
        .get_timestamp()
        .and_then(|ts| ts.as_timestamp())
        .map(|ts| (*ts).into())
        .unwrap_or_else(SystemTime::now);

    // Extract message
    let body = log
        .get_message()
        .map(|msg| msg.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Extract level
    let level = log
        .get("level")
        .or_else(|| log.get("severity"))
        .or_else(|| log.get("sentry.level"))
        .or_else(|| log.get("sentry.severity"))
        .map(
            |level_value| match level_value.to_string_lossy().to_lowercase().as_str() {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" | "warning" => LogLevel::Warn,
                "error" | "err" => LogLevel::Error,
                "fatal" | "critical" | "alert" | "emergency" => LogLevel::Fatal,
                _ => LogLevel::Info,
            },
        )
        .unwrap_or(LogLevel::Info);

    // Extract trace ID and determine which field was used
    let (trace_id, used_trace_field) = extract_trace_id(log);

    // Convert fields to attributes
    let attributes = convert_fields_to_attributes(log, used_trace_field);

    Log {
        level,
        body,
        trace_id: Some(trace_id),
        timestamp,
        severity_number: None, // We could map this from level if needed
        attributes,
    }
}

/// Convert log event fields to Sentry log attributes, excluding specified fields.
///
/// See https://develop.sentry.dev/sdk/telemetry/logs/#log-envelope-item
fn convert_fields_to_attributes(
    log: &vector_lib::event::LogEvent,
    used_trace_field: Option<&str>,
) -> Map<String, LogAttribute> {
    let mut attributes = Map::new();
    if let Some(fields) = log.all_event_fields() {
        for (key, value) in fields {
            let key_str = key.as_str();
            if key_str != "message"
                && key_str != "level"
                && key_str != "severity"
                && key_str != "timestamp"
                && Some(key_str) != used_trace_field
            {
                let sentry_value = match value {
                    vrl::value::Value::Bytes(b) => {
                        Value::String(String::from_utf8_lossy(b).to_string())
                    }
                    vrl::value::Value::Integer(i) => Value::Number(serde_json::Number::from(*i)),
                    vrl::value::Value::Float(f) => {
                        // Ensure we're using 64-bit floating point as per Sentry protocol
                        let float_val = f.into_inner();
                        if let Some(n) = serde_json::Number::from_f64(float_val) {
                            Value::Number(n)
                        } else {
                            // If the float can't be represented as a JSON number, convert to string
                            Value::String(float_val.to_string())
                        }
                    }
                    vrl::value::Value::Boolean(b) => Value::Bool(*b),
                    _ => Value::String(value.to_string_lossy().to_string()),
                };
                attributes.insert(key_str.to_string(), LogAttribute(sentry_value));
            }
        }
    }
    attributes
}

async fn healthcheck(dsn: SensitiveString) -> crate::Result<()> {
    // If sentry or the dsn is not configured properly, the init
    // call will panic, cause the healthcheck to fail. Init
    // also validates the SDK's internal transport mechanism.
    let _guard = sentry::init(dsn.inner());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{LogLevel, TraceId};
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use vector_lib::event::LogEvent;
    use vrl::value::Value;

    #[test]
    fn test_extract_trace_id_with_trace_id_field() {
        let mut log = LogEvent::from("test message");
        let test_uuid = Uuid::new_v4();
        log.insert("trace_id", Value::from(test_uuid.to_string()));

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from(test_uuid.into_bytes()));
        assert_eq!(used_field, Some("trace_id"));
    }

    #[test]
    fn test_extract_trace_id_with_sentry_trace_id_field() {
        let mut log = LogEvent::from("test message");
        let test_uuid = Uuid::new_v4();
        log.insert("sentry.trace_id", Value::from(test_uuid.to_string()));

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from(test_uuid.into_bytes()));
        assert_eq!(used_field, Some("sentry.trace_id"));
    }

    #[test]
    fn test_extract_trace_id_precedence() {
        let mut log = LogEvent::from("test message");
        let trace_uuid = Uuid::new_v4();
        let sentry_uuid = Uuid::new_v4();

        // Add both fields, trace_id should take precedence
        log.insert("trace_id", Value::from(trace_uuid.to_string()));
        log.insert("sentry.trace_id", Value::from(sentry_uuid.to_string()));

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from(trace_uuid.into_bytes()));
        assert_eq!(used_field, Some("trace_id"));
    }

    #[test]
    fn test_extract_trace_id_invalid_uuid() {
        let mut log = LogEvent::from("test message");
        log.insert("trace_id", Value::from("not-a-uuid"));

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from([0u8; 16]));
        assert_eq!(used_field, None);
    }

    #[test]
    fn test_extract_trace_id_no_trace_fields() {
        let log = LogEvent::from("test message");

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from([0u8; 16]));
        assert_eq!(used_field, None);
    }

    #[test]
    fn test_convert_to_sentry_log_basic() {
        let mut log = LogEvent::from("test message");
        log.insert("level", Value::from("info"));

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.body, "test message");
        assert_eq!(sentry_log.level, LogLevel::Info);
        assert!(sentry_log.trace_id.is_some());
    }

    #[test]
    fn test_convert_to_sentry_log_all_levels() {
        let test_cases = vec![
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("info", LogLevel::Info),
            ("warn", LogLevel::Warn),
            ("warning", LogLevel::Warn),
            ("error", LogLevel::Error),
            ("err", LogLevel::Error),
            ("fatal", LogLevel::Fatal),
            ("critical", LogLevel::Fatal),
            ("alert", LogLevel::Fatal),
            ("emergency", LogLevel::Fatal),
            ("unknown", LogLevel::Info), // Default case
        ];

        for (level_str, expected_level) in test_cases {
            let mut log = LogEvent::from("test message");
            log.insert("level", Value::from(level_str));

            let sentry_log = convert_to_sentry_log(&log);

            assert_eq!(
                sentry_log.level, expected_level,
                "Failed for level: {}",
                level_str
            );
        }
    }

    #[test]
    fn test_convert_to_sentry_log_severity_field() {
        let mut log = LogEvent::from("test message");
        log.insert("severity", Value::from("error"));

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.level, LogLevel::Error);
    }

    #[test]
    fn test_convert_to_sentry_log_sentry_level_field() {
        let mut log = LogEvent::from("test message");
        log.insert("sentry.level", Value::from("warn"));

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.level, LogLevel::Warn);
    }

    #[test]
    fn test_convert_to_sentry_log_with_trace_id() {
        let mut log = LogEvent::from("test message");
        let test_uuid = Uuid::new_v4();
        log.insert("trace_id", Value::from(test_uuid.to_string()));

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(
            sentry_log.trace_id.unwrap(),
            TraceId::from(test_uuid.into_bytes())
        );
    }

    #[test]
    fn test_convert_to_sentry_log_no_message() {
        let log = LogEvent::from(BTreeMap::new());

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.body, "");
        assert_eq!(sentry_log.level, LogLevel::Info);
    }

    #[test]
    fn test_convert_fields_to_attributes_excludes_reserved_fields() {
        let mut log = LogEvent::from("test message");
        log.insert("level", Value::from("info"));
        log.insert("severity", Value::from("high"));
        log.insert("timestamp", Value::from("2023-01-01T00:00:00Z"));
        log.insert("trace_id", Value::from("some-trace-id"));
        log.insert("custom_field", Value::from("custom_value"));

        let attributes = convert_fields_to_attributes(&log, Some("trace_id"));

        // Should only contain custom_field, not the reserved fields
        assert_eq!(attributes.len(), 1);
        assert!(attributes.contains_key("custom_field"));
        assert!(!attributes.contains_key("message"));
        assert!(!attributes.contains_key("level"));
        assert!(!attributes.contains_key("severity"));
        assert!(!attributes.contains_key("timestamp"));
        assert!(!attributes.contains_key("trace_id"));
    }

    #[test]
    fn test_convert_fields_to_attributes_different_types() {
        // Start with an empty log to avoid any default fields
        let mut log = LogEvent::from(BTreeMap::new());
        log.insert("string_field", Value::from("test_string"));
        log.insert("int_field", Value::from(42i64));
        log.insert("float_field", Value::from(3.14f64));
        log.insert("bool_field", Value::from(true));
        // Use a simple bytes value that converts to a string rather than an array
        log.insert("bytes_field", Value::from("test_bytes"));

        let attributes = convert_fields_to_attributes(&log, None);

        // Should have 5 attributes
        assert_eq!(attributes.len(), 5);

        // Check that our fields are present
        assert!(attributes.contains_key("string_field"));
        assert!(attributes.contains_key("int_field"));
        assert!(attributes.contains_key("float_field"));
        assert!(attributes.contains_key("bool_field"));
        assert!(attributes.contains_key("bytes_field"));

        // Check string field
        if let Some(attr) = attributes.get("string_field") {
            match &attr.0 {
                sentry::protocol::Value::String(s) => assert_eq!(s, "test_string"),
                _ => panic!("Expected string value"),
            }
        }

        // Check integer field
        if let Some(attr) = attributes.get("int_field") {
            match &attr.0 {
                sentry::protocol::Value::Number(n) => assert_eq!(n.as_i64(), Some(42)),
                _ => panic!("Expected number value"),
            }
        }

        // Check float field
        if let Some(attr) = attributes.get("float_field") {
            match &attr.0 {
                sentry::protocol::Value::Number(n) => assert_eq!(n.as_f64(), Some(3.14)),
                _ => panic!("Expected number value"),
            }
        }

        // Check boolean field
        if let Some(attr) = attributes.get("bool_field") {
            match &attr.0 {
                sentry::protocol::Value::Bool(b) => assert_eq!(*b, true),
                _ => panic!("Expected boolean value"),
            }
        }

        // Check bytes field (should be a string)
        if let Some(attr) = attributes.get("bytes_field") {
            match &attr.0 {
                sentry::protocol::Value::String(s) => assert_eq!(s, "test_bytes"),
                _ => panic!("Expected string value for bytes"),
            }
        }
    }

    #[test]
    fn test_convert_fields_to_attributes_bytes_array() {
        // Test that byte arrays are properly converted to string
        let mut log = LogEvent::from(BTreeMap::new());
        log.insert("bytes_field", Value::from(b"test_bytes".to_vec()));

        let attributes = convert_fields_to_attributes(&log, None);

        // Byte array gets expanded to individual indexed fields
        assert!(attributes.len() > 1);

        // Check that the first few byte fields exist and are numbers
        for i in 0..5 {
            let key = format!("bytes_field[{}]", i);
            assert!(attributes.contains_key(&key), "Missing key: {}", key);

            if let Some(attr) = attributes.get(&key) {
                match &attr.0 {
                    sentry::protocol::Value::Number(_) => {} // Expected
                    _ => panic!("Expected number value for byte at index {}", i),
                }
            }
        }
    }

    #[test]
    fn test_convert_fields_to_attributes_no_fields() {
        let log = LogEvent::from("test message");

        let attributes = convert_fields_to_attributes(&log, None);

        // Should be empty since we only have the message field which is excluded
        assert_eq!(attributes.len(), 0);
    }

    #[test]
    fn test_convert_fields_to_attributes_special_float_values() {
        // Start with an empty log to avoid any default fields
        let mut log = LogEvent::from(BTreeMap::new());
        log.insert("normal_float", Value::from(1.23f64));
        log.insert("infinity", Value::from(f64::INFINITY));
        log.insert("neg_infinity", Value::from(f64::NEG_INFINITY));
        // Note: NaN is not supported by VRL so we don't test it

        let attributes = convert_fields_to_attributes(&log, None);

        assert_eq!(attributes.len(), 3);

        // Normal float should be a number
        if let Some(attr) = attributes.get("normal_float") {
            assert!(matches!(attr.0, sentry::protocol::Value::Number(_)));
        }

        // Special float values should be converted to strings
        if let Some(attr) = attributes.get("infinity") {
            assert!(matches!(attr.0, sentry::protocol::Value::String(_)));
        }

        if let Some(attr) = attributes.get("neg_infinity") {
            assert!(matches!(attr.0, sentry::protocol::Value::String(_)));
        }
    }
}

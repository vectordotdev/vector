//! Vector log event to Sentry log conversion utilities.

use sentry::protocol::{Log, LogAttribute, LogLevel, Map, TraceId, Value};
use std::time::SystemTime;

use super::constants::{SDK_NAME, SDK_VERSION};

/// Extract trace ID from log event, returning the trace ID and which field was used.
pub fn extract_trace_id(log: &vector_lib::event::LogEvent) -> (TraceId, Option<&'static str>) {
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
pub fn convert_to_sentry_log(log: &vector_lib::event::LogEvent) -> Log {
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
pub fn convert_fields_to_attributes(
    log: &vector_lib::event::LogEvent,
    used_trace_field: Option<&str>,
) -> Map<String, LogAttribute> {
    let mut attributes = Map::new();

    attributes.insert(
        "sentry.sdk.name".to_string(),
        LogAttribute(Value::String(SDK_NAME.to_string())),
    );
    attributes.insert(
        "sentry.sdk.version".to_string(),
        LogAttribute(Value::String(SDK_VERSION.to_string())),
    );

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

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{LogLevel, TraceId};
    use vector_lib::event::LogEvent;
    use vrl::value::Value;

    fn create_test_log_event() -> LogEvent {
        let mut log = LogEvent::from("test message");
        log.insert("level", "info");
        log.insert("custom_field", "custom_value");
        log
    }

    #[test]
    fn test_extract_trace_id_from_trace_id_field() {
        let mut log = LogEvent::from("test");
        log.insert("trace_id", "550e8400-e29b-41d4-a716-446655440000");

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_ne!(trace_id, TraceId::from([0u8; 16])); // Not the default zero trace ID
        assert_eq!(used_field, Some("trace_id"));
    }

    #[test]
    fn test_extract_trace_id_from_sentry_trace_id_field() {
        let mut log = LogEvent::from("test");
        log.insert("sentry.trace_id", "550e8400-e29b-41d4-a716-446655440000");

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_ne!(trace_id, TraceId::from([0u8; 16])); // Not the default zero trace ID
        assert_eq!(used_field, Some("sentry.trace_id"));
    }

    #[test]
    fn test_extract_trace_id_invalid_uuid() {
        let mut log = LogEvent::from("test");
        log.insert("trace_id", "invalid-uuid-format");

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from([0u8; 16])); // Should be default zero trace ID
        assert_eq!(used_field, None);
    }

    #[test]
    fn test_extract_trace_id_missing() {
        let log = LogEvent::from("test");

        let (trace_id, used_field) = extract_trace_id(&log);

        assert_eq!(trace_id, TraceId::from([0u8; 16])); // Should be default zero trace ID
        assert_eq!(used_field, None);
    }

    #[test]
    fn test_extract_trace_id_priority() {
        let mut log = LogEvent::from("test");
        log.insert("trace_id", "550e8400-e29b-41d4-a716-446655440000");
        log.insert("sentry.trace_id", "123e4567-e89b-12d3-a456-426614174000");

        let (trace_id, used_field) = extract_trace_id(&log);

        // Should prefer "trace_id" field first
        assert_ne!(trace_id, TraceId::from([0u8; 16]));
        assert_eq!(used_field, Some("trace_id"));
    }

    #[test]
    fn test_convert_to_sentry_log_basic() {
        let log = create_test_log_event();

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.body, "test message");
        assert_eq!(sentry_log.level, LogLevel::Info);
        assert!(sentry_log.trace_id.is_some());
        assert!(sentry_log.attributes.contains_key("sentry.sdk.name"));
        assert!(sentry_log.attributes.contains_key("sentry.sdk.version"));
        assert!(sentry_log.attributes.contains_key("custom_field"));
    }

    #[test]
    fn test_convert_to_sentry_log_levels() {
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
            ("unknown", LogLevel::Info), // Default fallback
        ];

        for (level_str, expected_level) in test_cases {
            let mut log = LogEvent::from("test");
            log.insert("level", level_str);

            let sentry_log = convert_to_sentry_log(&log);

            assert_eq!(
                sentry_log.level, expected_level,
                "Failed for level: {}",
                level_str
            );
        }
    }

    #[test]
    fn test_convert_to_sentry_log_level_field_priority() {
        let mut log = LogEvent::from("test");
        log.insert("level", "error");
        log.insert("severity", "warn");
        log.insert("sentry.level", "debug");
        log.insert("sentry.severity", "info");

        let sentry_log = convert_to_sentry_log(&log);

        // Should prefer "level" field first
        assert_eq!(sentry_log.level, LogLevel::Error);
    }

    #[test]
    fn test_convert_to_sentry_log_no_level() {
        let log = LogEvent::from("test");

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.level, LogLevel::Info); // Default fallback
    }

    #[test]
    fn test_convert_to_sentry_log_no_message() {
        let mut log = LogEvent::default();
        log.insert("level", "info");

        let sentry_log = convert_to_sentry_log(&log);

        assert_eq!(sentry_log.body, ""); // Should be empty string
    }

    #[test]
    fn test_convert_to_sentry_log_with_trace_id() {
        let mut log = LogEvent::from("test");
        log.insert("trace_id", "550e8400-e29b-41d4-a716-446655440000");

        let sentry_log = convert_to_sentry_log(&log);

        assert!(sentry_log.trace_id.is_some());
        assert_ne!(sentry_log.trace_id.unwrap(), TraceId::from([0u8; 16]));
        // trace_id field should be excluded from attributes
        assert!(!sentry_log.attributes.contains_key("trace_id"));
    }

    #[test]
    fn test_convert_fields_to_attributes_basic() {
        let mut log = LogEvent::from("test message");
        log.insert("custom_string", "value");
        log.insert("custom_number", 42);
        log.insert("custom_bool", true);
        log.insert("custom_float", 3.14);

        let attributes = convert_fields_to_attributes(&log, None);

        // Check SDK attributes are present
        assert!(attributes.contains_key("sentry.sdk.name"));
        assert!(attributes.contains_key("sentry.sdk.version"));

        // Check custom fields are converted
        assert!(attributes.contains_key("custom_string"));
        assert!(attributes.contains_key("custom_number"));
        assert!(attributes.contains_key("custom_bool"));
        assert!(attributes.contains_key("custom_float"));

        // Check reserved fields are excluded
        assert!(!attributes.contains_key("message"));
    }

    #[test]
    fn test_convert_fields_to_attributes_excluded_fields() {
        let mut log = LogEvent::from("test message");
        log.insert("level", "info");
        log.insert("severity", "error");
        log.insert("timestamp", "2023-01-01T00:00:00Z");
        log.insert("trace_id", "550e8400-e29b-41d4-a716-446655440000");
        log.insert("custom_field", "should_be_included");

        let attributes = convert_fields_to_attributes(&log, Some("trace_id"));

        // Reserved fields should be excluded
        assert!(!attributes.contains_key("message"));
        assert!(!attributes.contains_key("level"));
        assert!(!attributes.contains_key("severity"));
        assert!(!attributes.contains_key("timestamp"));
        assert!(!attributes.contains_key("trace_id"));

        // Custom field should be included
        assert!(attributes.contains_key("custom_field"));

        // SDK attributes should always be present
        assert!(attributes.contains_key("sentry.sdk.name"));
        assert!(attributes.contains_key("sentry.sdk.version"));
    }

    #[test]
    fn test_convert_fields_to_attributes_value_types() {
        let mut log = LogEvent::default();
        log.insert("string_field", "text_value");
        log.insert("int_field", 123);
        log.insert("float_field", 45.67);
        log.insert("bool_field", false);
        log.insert("bytes_field", Value::Bytes("hello".into()));

        let attributes = convert_fields_to_attributes(&log, None);

        // Verify values are properly converted to Sentry Value types
        if let Some(attr) = attributes.get("string_field") {
            assert!(matches!(attr.0, sentry::protocol::Value::String(_)));
        }
        if let Some(attr) = attributes.get("int_field") {
            assert!(matches!(attr.0, sentry::protocol::Value::Number(_)));
        }
        if let Some(attr) = attributes.get("float_field") {
            assert!(matches!(attr.0, sentry::protocol::Value::Number(_)));
        }
        if let Some(attr) = attributes.get("bool_field") {
            assert!(matches!(attr.0, sentry::protocol::Value::Bool(false)));
        }
        if let Some(attr) = attributes.get("bytes_field") {
            assert!(matches!(attr.0, sentry::protocol::Value::String(_)));
        }
    }

    #[test]
    fn test_convert_fields_to_attributes_float_edge_cases() {
        let mut log = LogEvent::default();
        // Note: VRL doesn't allow NaN values to be inserted, so we test with other edge cases
        log.insert("inf_float", f64::INFINITY);
        log.insert("neg_inf_float", f64::NEG_INFINITY);
        log.insert("large_float", f64::MAX);
        log.insert("small_float", f64::MIN);

        let attributes = convert_fields_to_attributes(&log, None);

        // Infinity and other edge case float values should be handled
        assert!(attributes.contains_key("inf_float"));
        assert!(attributes.contains_key("neg_inf_float"));
        assert!(attributes.contains_key("large_float"));
        assert!(attributes.contains_key("small_float"));
    }

    #[test]
    fn test_convert_fields_to_attributes_sdk_values() {
        let log = LogEvent::default();

        let attributes = convert_fields_to_attributes(&log, None);

        // Check SDK name and version are set correctly
        if let Some(sdk_name) = attributes.get("sentry.sdk.name") {
            if let sentry::protocol::Value::String(name) = &sdk_name.0 {
                assert_eq!(name, SDK_NAME);
            } else {
                panic!("SDK name should be a string");
            }
        } else {
            panic!("SDK name should be present");
        }

        if let Some(sdk_version) = attributes.get("sentry.sdk.version") {
            if let sentry::protocol::Value::String(version) = &sdk_version.0 {
                assert_eq!(version, SDK_VERSION);
            } else {
                panic!("SDK version should be a string");
            }
        } else {
            panic!("SDK version should be present");
        }
    }

    #[test]
    fn test_convert_fields_to_attributes_empty_log() {
        let log = LogEvent::default();

        let attributes = convert_fields_to_attributes(&log, None);

        // Should still have SDK attributes even for empty log
        assert!(attributes.contains_key("sentry.sdk.name"));
        assert!(attributes.contains_key("sentry.sdk.version"));
        assert_eq!(attributes.len(), 2); // Only SDK attributes
    }
}

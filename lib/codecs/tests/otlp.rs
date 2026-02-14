//! Integration tests for OTLP serializer with native log conversion.
//!
//! Test structure follows protobuf.rs pattern:
//! - Helper functions for setup
//! - Roundtrip tests
//! - Edge case tests

#![allow(clippy::unwrap_used)]

use bytes::BytesMut;
use chrono::Utc;
use codecs::encoding::{OtlpSerializer, OtlpSerializerConfig};
use opentelemetry_proto::proto::collector::logs::v1::ExportLogsServiceRequest;
use prost::Message;
use tokio_util::codec::Encoder;
use vector_core::event::{Event, EventMetadata, LogEvent};
use vrl::btreemap;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn build_serializer() -> OtlpSerializer {
    OtlpSerializerConfig::default().build().unwrap()
}

fn encode_log(log: LogEvent) -> BytesMut {
    let mut serializer = build_serializer();
    let mut buffer = BytesMut::new();
    serializer.encode(Event::Log(log), &mut buffer).unwrap();
    buffer
}

fn encode_and_decode(log: LogEvent) -> ExportLogsServiceRequest {
    let buffer = encode_log(log);
    ExportLogsServiceRequest::decode(&buffer[..]).unwrap()
}

// ============================================================================
// BASIC FUNCTIONALITY TESTS
// ============================================================================

#[test]
fn test_native_log_encoding_basic() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "severity_text" => "INFO",
        "severity_number" => 9i64,
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    assert_eq!(
        request.resource_logs.len(),
        1,
        "Should have one ResourceLogs"
    );

    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];
    assert_eq!(lr.severity_text, "INFO");
    assert_eq!(lr.severity_number, 9);
    assert!(lr.body.is_some());
}

#[test]
fn test_native_log_with_attributes() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "attributes" => btreemap! {
            "app" => "test-app",
            "version" => "1.0.0",
            "count" => 42i64,
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert_eq!(lr.attributes.len(), 3);
}

#[test]
fn test_native_log_with_resources() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "resources" => btreemap! {
            "service.name" => "test-service",
            "host.name" => "test-host",
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let resource = request.resource_logs[0].resource.as_ref().unwrap();

    assert_eq!(resource.attributes.len(), 2);
}

#[test]
fn test_native_log_with_scope() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "scope" => btreemap! {
            "name" => "test-scope",
            "version" => "1.0.0",
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let scope = request.resource_logs[0].scope_logs[0]
        .scope
        .as_ref()
        .unwrap();

    assert_eq!(scope.name, "test-scope");
    assert_eq!(scope.version, "1.0.0");
}

#[test]
fn test_native_log_with_trace_context() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "trace_id" => "0123456789abcdef0123456789abcdef",
        "span_id" => "0123456789abcdef",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert_eq!(lr.trace_id.len(), 16);
    assert_eq!(lr.span_id.len(), 8);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_empty_log_produces_valid_otlp() {
    let log = LogEvent::default();
    let mut serializer = build_serializer();
    let mut buffer = BytesMut::new();

    // Should succeed, not error
    serializer.encode(Event::Log(log), &mut buffer).unwrap();

    // Should be decodable
    let request = ExportLogsServiceRequest::decode(&buffer[..]).unwrap();
    assert_eq!(request.resource_logs.len(), 1);
}

#[test]
fn test_invalid_trace_id_handled() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "trace_id" => "not-valid-hex",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    // Should not panic
    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Invalid trace_id should result in empty
    assert!(lr.trace_id.is_empty());
}

#[test]
fn test_invalid_span_id_handled() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "span_id" => "zzzz",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Invalid span_id should result in empty
    assert!(lr.span_id.is_empty());
}

#[test]
fn test_severity_number_clamped() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "severity_number" => 100i64, // Out of range (max is 24)
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Should be clamped to max
    assert_eq!(lr.severity_number, 24);
}

#[test]
fn test_negative_timestamp_uses_zero() {
    let event_fields = btreemap! {
        "message" => "Test message",
        "timestamp" => -1i64,
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Negative timestamp should default to 0
    assert_eq!(lr.time_unix_nano, 0);
}

// ============================================================================
// SOURCE COMPATIBILITY TESTS
// ============================================================================

#[test]
fn test_file_source_json_log() {
    // Simulate a log from file source with JSON
    let event_fields = btreemap! {
        "message" => "User logged in",
        "level" => "info",
        "user_id" => "12345",
        "request_id" => "abc-123",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Message should be in body
    assert!(lr.body.is_some());
}

#[test]
fn test_syslog_source_log() {
    // Simulate a parsed syslog message
    let event_fields = btreemap! {
        "message" => "sshd[1234]: Accepted password for user",
        "severity_text" => "INFO",
        "attributes" => btreemap! {
            "facility" => "auth",
            "hostname" => "server01",
            "appname" => "sshd",
            "procid" => "1234",
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
    assert_eq!(lr.attributes.len(), 4);
}

#[test]
fn test_modified_otlp_passthrough() {
    // User received OTLP, modified it, and is sending it back
    // with use_otlp_decoding: false (flat format)
    let event_fields = btreemap! {
        "message" => "Original OTLP log",
        "severity_text" => "ERROR",
        "severity_number" => 17i64,
        "trace_id" => "0123456789abcdef0123456789abcdef",
        "span_id" => "0123456789abcdef",
        "flags" => 1i64,
        "dropped_attributes_count" => 2i64,
        "attributes" => btreemap! {
            "original" => "value",
            "added_by_transform" => "new_value",
        },
        "resources" => btreemap! {
            "service.name" => "my-service",
        },
        "scope" => btreemap! {
            "name" => "my-scope",
            "version" => "1.0",
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // All fields should be preserved
    assert_eq!(lr.severity_text, "ERROR");
    assert_eq!(lr.severity_number, 17);
    assert_eq!(lr.trace_id.len(), 16);
    assert_eq!(lr.span_id.len(), 8);
    assert_eq!(lr.flags, 1);
    assert_eq!(lr.dropped_attributes_count, 2);
    assert_eq!(lr.attributes.len(), 2);

    let scope = request.resource_logs[0].scope_logs[0]
        .scope
        .as_ref()
        .unwrap();
    assert_eq!(scope.name, "my-scope");
    assert_eq!(scope.version, "1.0");

    let resource = request.resource_logs[0].resource.as_ref().unwrap();
    assert!(!resource.attributes.is_empty());
}

// ============================================================================
// TIMESTAMP HANDLING TESTS
// ============================================================================

#[test]
fn test_timestamp_as_seconds() {
    let event_fields = btreemap! {
        "message" => "Test",
        "timestamp" => 1704067200i64, // 2024-01-01 00:00:00 UTC in seconds
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Should convert to nanoseconds
    assert_eq!(lr.time_unix_nano, 1704067200_000_000_000u64);
}

#[test]
fn test_timestamp_as_nanos() {
    let event_fields = btreemap! {
        "message" => "Test",
        "timestamp" => 1704067200_000_000_000i64, // Already in nanos
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert_eq!(lr.time_unix_nano, 1704067200_000_000_000u64);
}

#[test]
fn test_timestamp_as_chrono() {
    let mut log = LogEvent::default();
    let ts = Utc::now();
    log.insert("message", "Test");
    log.insert("timestamp", ts);

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.time_unix_nano > 0);
}

#[test]
fn test_timestamp_as_rfc3339_string() {
    let event_fields = btreemap! {
        "message" => "Test",
        "timestamp" => "2024-01-01T00:00:00Z",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.time_unix_nano > 0);
}

// ============================================================================
// SEVERITY INFERENCE TESTS
// ============================================================================

#[test]
fn test_severity_inferred_from_text_error() {
    let event_fields = btreemap! {
        "message" => "Test",
        "severity_text" => "ERROR",
        // No severity_number set
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Should infer severity number from text
    assert_eq!(lr.severity_number, 17); // SeverityNumber::Error
}

#[test]
fn test_severity_inferred_from_text_warn() {
    let event_fields = btreemap! {
        "message" => "Test",
        "severity_text" => "WARNING",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert_eq!(lr.severity_number, 13); // SeverityNumber::Warn
}

#[test]
fn test_severity_inferred_from_text_debug() {
    let event_fields = btreemap! {
        "message" => "Test",
        "severity_text" => "DEBUG",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert_eq!(lr.severity_number, 5); // SeverityNumber::Debug
}

// ============================================================================
// MESSAGE FIELD FALLBACK TESTS
// ============================================================================

#[test]
fn test_body_from_message_field() {
    let event_fields = btreemap! {
        "message" => "From message field",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
}

#[test]
fn test_body_from_body_field() {
    let event_fields = btreemap! {
        "body" => "From body field",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
}

#[test]
fn test_body_from_msg_field() {
    let event_fields = btreemap! {
        "msg" => "From msg field",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
}

#[test]
fn test_body_from_log_field() {
    let event_fields = btreemap! {
        "log" => "From log field",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
}

#[test]
fn test_message_takes_priority_over_body() {
    // When both message and body exist, message should be used
    let event_fields = btreemap! {
        "message" => "From message",
        "body" => "From body",
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    assert!(lr.body.is_some());
    // The body should contain "From message" since message has priority
    let body = lr.body.as_ref().unwrap();
    let body_value = body.value.as_ref().unwrap();
    match body_value {
        opentelemetry_proto::proto::common::v1::any_value::Value::StringValue(s) => {
            assert_eq!(s, "From message");
        }
        _ => panic!("Expected StringValue body"),
    }
}

// ============================================================================
// ROUNDTRIP TESTS
// ============================================================================

#[test]
fn test_encode_produces_valid_protobuf() {
    let event_fields = btreemap! {
        "message" => "Roundtrip test",
        "severity_text" => "WARN",
        "severity_number" => 13i64,
        "attributes" => btreemap! {
            "key1" => "value1",
            "key2" => 42i64,
            "key3" => true,
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let buffer = encode_log(log);

    // Verify it decodes correctly
    let request = ExportLogsServiceRequest::decode(&buffer[..]).unwrap();
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Verify body
    let body = lr.body.as_ref().unwrap().value.as_ref().unwrap();
    match body {
        opentelemetry_proto::proto::common::v1::any_value::Value::StringValue(s) => {
            assert_eq!(s, "Roundtrip test");
        }
        _ => panic!("Expected StringValue body"),
    }

    // Verify attributes with correct types
    assert_eq!(lr.attributes.len(), 3);
}

// ============================================================================
// MIXED VALID/INVALID FIELDS TEST
// ============================================================================

#[test]
fn test_mixed_valid_invalid_fields() {
    let event_fields = btreemap! {
        "message" => "Valid message",
        "timestamp" => -999i64, // Invalid
        "severity_number" => 9i64, // Valid
        "trace_id" => "not-hex", // Invalid
        "attributes" => btreemap! {
            "valid" => "value",
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Valid fields should be present
    assert!(lr.body.is_some());
    assert_eq!(lr.severity_number, 9);
    assert!(!lr.attributes.is_empty());

    // Invalid fields should have safe defaults
    assert_eq!(lr.time_unix_nano, 0);
    assert!(lr.trace_id.is_empty());
}

// ============================================================================
// COMPLEX ATTRIBUTE TYPES TEST
// ============================================================================

#[test]
fn test_nested_attributes() {
    let event_fields = btreemap! {
        "message" => "Test",
        "attributes" => btreemap! {
            "string_attr" => "value",
            "int_attr" => 42i64,
            "float_attr" => 3.14f64,
            "bool_attr" => true,
            "array_attr" => vec![1i64, 2i64, 3i64],
            "nested_attr" => btreemap! {
                "inner_key" => "inner_value",
            },
        },
    };
    let log = LogEvent::from_map(event_fields, EventMetadata::default());

    let request = encode_and_decode(log);
    let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

    // Should have all 6 attributes
    assert_eq!(lr.attributes.len(), 6);
}

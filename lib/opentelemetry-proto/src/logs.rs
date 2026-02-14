use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use tracing::warn;
use vector_core::{
    config::{LegacyKey, LogNamespace, log_schema},
    event::{Event, LogEvent},
};
use vrl::{core::Value, path};

use super::common::{
    from_hex, kv_list_into_value, to_hex, validate_span_id, validate_trace_id,
    value_object_to_kv_list,
};
use crate::proto::{
    collector::logs::v1::ExportLogsServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value::Value as PBValue},
    logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber},
    resource::v1::Resource,
};

const SOURCE_NAME: &str = "opentelemetry";
pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";
pub const SCOPE_KEY: &str = "scope";
pub const NAME_KEY: &str = "name";
pub const VERSION_KEY: &str = "version";
pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const SEVERITY_TEXT_KEY: &str = "severity_text";
pub const SEVERITY_NUMBER_KEY: &str = "severity_number";
pub const OBSERVED_TIMESTAMP_KEY: &str = "observed_timestamp";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const FLAGS_KEY: &str = "flags";

impl ResourceLogs {
    pub fn into_event_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let now = Utc::now();

        self.scope_logs.into_iter().flat_map(move |scope_log| {
            let scope = scope_log.scope;
            let resource = self.resource.clone();
            scope_log.log_records.into_iter().map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    log_record,
                }
                .into_event(log_namespace, now)
            })
        })
    }
}

struct ResourceLog {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    log_record: LogRecord,
}

// https://github.com/open-telemetry/opentelemetry-specification/blob/v1.15.0/specification/logs/data-model.md
impl ResourceLog {
    fn into_event(self, log_namespace: LogNamespace, now: DateTime<Utc>) -> Event {
        let mut log = match log_namespace {
            LogNamespace::Vector => {
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    LogEvent::from(<PBValue as Into<Value>>::into(v))
                } else {
                    LogEvent::from(Value::Null)
                }
            }
            LogNamespace::Legacy => {
                let mut log = LogEvent::default();
                if let Some(v) = self.log_record.body.and_then(|av| av.value) {
                    log.maybe_insert(log_schema().message_key_target_path(), v);
                }
                log
            }
        };

        // Insert instrumentation scope (scope name, version, and attributes)
        if let Some(scope) = self.scope {
            if !scope.name.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, NAME_KEY))),
                    path!(SCOPE_KEY, NAME_KEY),
                    scope.name,
                );
            }
            if !scope.version.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, VERSION_KEY))),
                    path!(SCOPE_KEY, VERSION_KEY),
                    scope.version,
                );
            }
            if !scope.attributes.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(SCOPE_KEY, ATTRIBUTES_KEY))),
                    path!(SCOPE_KEY, ATTRIBUTES_KEY),
                    kv_list_into_value(scope.attributes),
                );
            }
            if scope.dropped_attributes_count > 0 {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(
                        SCOPE_KEY,
                        DROPPED_ATTRIBUTES_COUNT_KEY
                    ))),
                    path!(SCOPE_KEY, DROPPED_ATTRIBUTES_COUNT_KEY),
                    scope.dropped_attributes_count,
                );
            }
        }

        // Optional fields
        if let Some(resource) = self.resource
            && !resource.attributes.is_empty()
        {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(RESOURCE_KEY))),
                path!(RESOURCE_KEY),
                kv_list_into_value(resource.attributes),
            );
        }
        if !self.log_record.attributes.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(ATTRIBUTES_KEY))),
                path!(ATTRIBUTES_KEY),
                kv_list_into_value(self.log_record.attributes),
            );
        }
        if !self.log_record.trace_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(TRACE_ID_KEY))),
                path!(TRACE_ID_KEY),
                Bytes::from(to_hex(&self.log_record.trace_id)),
            );
        }
        if !self.log_record.span_id.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SPAN_ID_KEY))),
                path!(SPAN_ID_KEY),
                Bytes::from(to_hex(&self.log_record.span_id)),
            );
        }
        if !self.log_record.severity_text.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SEVERITY_TEXT_KEY))),
                path!(SEVERITY_TEXT_KEY),
                self.log_record.severity_text,
            );
        }
        if self.log_record.severity_number != SeverityNumber::Unspecified as i32 {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SEVERITY_NUMBER_KEY))),
                path!(SEVERITY_NUMBER_KEY),
                self.log_record.severity_number,
            );
        }
        if self.log_record.flags > 0 {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(FLAGS_KEY))),
                path!(FLAGS_KEY),
                self.log_record.flags,
            );
        }

        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(DROPPED_ATTRIBUTES_COUNT_KEY))),
            path!(DROPPED_ATTRIBUTES_COUNT_KEY),
            self.log_record.dropped_attributes_count,
        );

        // According to log data model spec, if observed_time_unix_nano is missing, the collector
        // should set it to the current time.
        let observed_timestamp = if self.log_record.observed_time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.observed_time_unix_nano as i64)
                .into()
        } else {
            Value::Timestamp(now)
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(OBSERVED_TIMESTAMP_KEY))),
            path!(OBSERVED_TIMESTAMP_KEY),
            observed_timestamp.clone(),
        );

        // If time_unix_nano is not present (0 represents missing or unknown timestamp) use observed time
        let timestamp = if self.log_record.time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.time_unix_nano as i64)
                .into()
        } else {
            observed_timestamp
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            log_schema().timestamp_key().map(LegacyKey::Overwrite),
            path!("timestamp"),
            timestamp,
        );

        log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            path!("source_type"),
            Bytes::from_static(SOURCE_NAME.as_bytes()),
        );
        if log_namespace == LogNamespace::Vector {
            log.metadata_mut()
                .value_mut()
                .insert(path!("vector", "ingest_timestamp"), now);
        }

        log.into()
    }
}

// ============================================================================
// Native Vector Log → OTLP Conversion
// ============================================================================

/// Convert a native Vector LogEvent to OTLP ExportLogsServiceRequest.
///
/// This function handles events from any source:
/// - OTLP receiver with `use_otlp_decoding: false` (flat decoded OTLP)
/// - File source with JSON logs
/// - Any other Vector source (socket, kafka, etc.)
/// - User-modified events with potentially invalid data
///
/// Invalid fields are handled gracefully with defaults/warnings, not errors.
pub fn native_log_to_otlp_request(log: &LogEvent) -> ExportLogsServiceRequest {
    let log_record = build_log_record_from_native(log);
    let scope_logs = build_scope_logs_from_native(log, log_record);
    let resource_logs = build_resource_logs_from_native(log, scope_logs);

    ExportLogsServiceRequest {
        resource_logs: vec![resource_logs],
    }
}

fn build_log_record_from_native(log: &LogEvent) -> LogRecord {
    LogRecord {
        time_unix_nano: extract_timestamp_nanos_safe(log, "timestamp"),
        observed_time_unix_nano: extract_timestamp_nanos_safe(log, OBSERVED_TIMESTAMP_KEY),
        severity_number: extract_severity_number_safe(log),
        severity_text: extract_string_safe(log, SEVERITY_TEXT_KEY),
        body: extract_body_safe(log),
        attributes: extract_kv_attributes_safe(log, ATTRIBUTES_KEY),
        dropped_attributes_count: extract_u32_safe(log, DROPPED_ATTRIBUTES_COUNT_KEY),
        flags: extract_u32_safe(log, FLAGS_KEY),
        trace_id: extract_trace_id_safe(log),
        span_id: extract_span_id_safe(log),
    }
}

fn build_scope_logs_from_native(log: &LogEvent, log_record: LogRecord) -> ScopeLogs {
    ScopeLogs {
        scope: extract_instrumentation_scope_safe(log),
        log_records: vec![log_record],
        schema_url: String::new(),
    }
}

fn build_resource_logs_from_native(log: &LogEvent, scope_logs: ScopeLogs) -> ResourceLogs {
    ResourceLogs {
        resource: extract_resource_safe(log),
        scope_logs: vec![scope_logs],
        schema_url: String::new(),
    }
}

// ============================================================================
// Safe extraction helpers - reuse existing patterns from Vector
// ============================================================================

/// Extract timestamp as nanoseconds, handling multiple input formats.
fn extract_timestamp_nanos_safe(log: &LogEvent, key: &str) -> u64 {
    let value = match log.get(key) {
        Some(v) => v,
        None => return 0, // Missing timestamp is valid (0 means unset in OTLP)
    };

    match value {
        // Native timestamp - use existing chrono methods
        Value::Timestamp(ts) => ts.timestamp_nanos_opt().unwrap_or(0) as u64,
        // Integer - could be seconds or nanos (heuristic detection)
        Value::Integer(i) => {
            let i = *i;
            if i < 0 {
                warn!(
                    message = "Negative timestamp, using 0",
                    field = key,
                    value = i
                );
                return 0;
            }
            // Heuristic: year 2001 in nanos = 1e18, in seconds = 1e9
            // If value < 1 trillion, assume seconds; otherwise assume nanos
            if i < 1_000_000_000_000 {
                (i as u64) * 1_000_000_000 // seconds → nanos
            } else {
                i as u64 // already nanos
            }
        }
        // Float - could be fractional seconds
        Value::Float(f) => {
            let f = f.into_inner();
            if f < 0.0 || f.is_nan() {
                warn!(message = "Invalid float timestamp, using 0", field = key);
                return 0;
            }
            if f < 1e12 { (f * 1e9) as u64 } else { f as u64 }
        }
        // String - try RFC3339 or numeric
        Value::Bytes(b) => {
            let s = String::from_utf8_lossy(b);
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.timestamp_nanos_opt().unwrap_or(0) as u64)
                .or_else(|_| {
                    s.parse::<i64>().map(|ts| {
                        if ts < 1_000_000_000_000 {
                            (ts as u64) * 1_000_000_000
                        } else {
                            ts as u64
                        }
                    })
                })
                .unwrap_or_else(|_| {
                    warn!(
                        message = "Could not parse timestamp string",
                        field = key,
                        value = %s
                    );
                    0
                })
        }
        _ => {
            warn!(message = "Unexpected timestamp type", field = key);
            0
        }
    }
}

/// Extract string field, handling multiple types.
#[inline]
fn extract_string_safe(log: &LogEvent, key: &str) -> String {
    match log.get(key) {
        Some(Value::Bytes(b)) => {
            // Optimization: try valid UTF-8 first to avoid extra allocation
            String::from_utf8(b.to_vec()).unwrap_or_else(|e| {
                String::from_utf8_lossy(e.as_bytes()).into_owned()
            })
        }
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Boolean(b)) => if *b { "true" } else { "false" }.to_string(),
        Some(other) => {
            warn!(
                message = "Converting non-string to string",
                field = key,
                value_type = ?other
            );
            format!("{other:?}")
        }
        None => String::new(),
    }
}

/// Extract severity number with validation.
fn extract_severity_number_safe(log: &LogEvent) -> i32 {
    let value = match log.get(SEVERITY_NUMBER_KEY) {
        Some(v) => v,
        None => {
            // Try to infer from severity_text if number not present
            return infer_severity_number(log);
        }
    };

    match value {
        Value::Integer(i) => {
            let i = *i;
            // OTLP severity numbers are 0-24
            if !(0..=24).contains(&i) {
                warn!(message = "Severity number out of range (0-24)", value = i);
                i.clamp(0, 24) as i32
            } else {
                i as i32
            }
        }
        Value::Bytes(b) => {
            // String number
            let s = String::from_utf8_lossy(b);
            s.parse::<i32>().unwrap_or_else(|_| {
                warn!(message = "Could not parse severity_number", value = %s);
                0
            })
        }
        _ => {
            warn!(
                message = "Unexpected severity_number type",
                value_type = ?value
            );
            0
        }
    }
}

/// Infer severity number from severity text.
fn infer_severity_number(log: &LogEvent) -> i32 {
    let text = match log.get(SEVERITY_TEXT_KEY) {
        Some(Value::Bytes(b)) => String::from_utf8_lossy(b).to_uppercase(),
        _ => return SeverityNumber::Unspecified as i32,
    };

    match text.as_str() {
        "TRACE" | "TRACE2" | "TRACE3" | "TRACE4" => SeverityNumber::Trace as i32,
        "DEBUG" | "DEBUG2" | "DEBUG3" | "DEBUG4" => SeverityNumber::Debug as i32,
        "INFO" | "INFO2" | "INFO3" | "INFO4" | "NOTICE" => SeverityNumber::Info as i32,
        "WARN" | "WARNING" | "WARN2" | "WARN3" | "WARN4" => SeverityNumber::Warn as i32,
        "ERROR" | "ERR" | "ERROR2" | "ERROR3" | "ERROR4" => SeverityNumber::Error as i32,
        "FATAL" | "CRITICAL" | "CRIT" | "EMERG" | "EMERGENCY" | "ALERT" => {
            SeverityNumber::Fatal as i32
        }
        _ => SeverityNumber::Unspecified as i32,
    }
}

/// Extract body, supporting various message field locations.
#[inline]
fn extract_body_safe(log: &LogEvent) -> Option<AnyValue> {
    // Priority order for finding the log body:
    // 1. .message (Legacy namespace standard)
    // 2. .body (explicit OTLP field name)
    // 3. .msg (common alternative)
    // 4. .log (some formats use this)
    // Static field names to avoid repeated string allocations
    const BODY_FIELDS: [&str; 4] = ["message", "body", "msg", "log"];

    for field in BODY_FIELDS {
        if let Some(v) = log.get(field) {
            return Some(AnyValue {
                value: Some(v.clone().into()),
            });
        }
    }
    None
}

/// Extract u32 field safely.
fn extract_u32_safe(log: &LogEvent, key: &str) -> u32 {
    match log.get(key) {
        Some(Value::Integer(i)) => {
            let i = *i;
            if i < 0 {
                warn!(
                    message = "Negative value for u32 field, using 0",
                    field = key,
                    value = i
                );
                0
            } else if i > u32::MAX as i64 {
                warn!(
                    message = "Value overflow for u32 field",
                    field = key,
                    value = i
                );
                u32::MAX
            } else {
                i as u32
            }
        }
        Some(Value::Bytes(b)) => {
            let s = String::from_utf8_lossy(b);
            s.parse::<u32>().unwrap_or(0)
        }
        _ => 0,
    }
}

/// Extract attributes object, handling nested structures.
#[inline]
fn extract_kv_attributes_safe(log: &LogEvent, key: &str) -> Vec<KeyValue> {
    match log.get(key) {
        Some(Value::Object(obj)) => {
            // Pre-allocate and convert without cloning when possible
            let mut result = Vec::with_capacity(obj.len());
            for (k, v) in obj.iter() {
                if matches!(v, Value::Null) {
                    continue;
                }
                result.push(KeyValue {
                    key: k.to_string(),
                    value: Some(AnyValue {
                        value: Some(v.clone().into()),
                    }),
                });
            }
            result
        }
        Some(Value::Array(arr)) => {
            // User might have stored pre-formatted KeyValue array
            let mut result = Vec::with_capacity(arr.len());
            for v in arr.iter() {
                if let Value::Object(obj) = v {
                    if let Some(key) = obj.get("key").and_then(|v| v.as_str()) {
                        result.push(KeyValue {
                            key: key.to_string(),
                            value: obj.get("value").map(|v| AnyValue {
                                value: Some(v.clone().into()),
                            }),
                        });
                    }
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

/// Extract trace_id with validation.
#[inline]
fn extract_trace_id_safe(log: &LogEvent) -> Vec<u8> {
    match log.get(TRACE_ID_KEY) {
        Some(Value::Bytes(b)) => {
            // Optimization: check if already valid 16-byte binary
            if b.len() == 16 {
                return b.to_vec();
            }
            // Otherwise treat as hex string
            // Try direct str conversion if ASCII (common case)
            let s = if b.is_ascii() {
                // Safety: we just checked it's ASCII
                unsafe { std::str::from_utf8_unchecked(b) }
            } else {
                return Vec::new(); // Invalid hex
            };
            from_hex(s)
        }
        Some(Value::Array(arr)) => {
            // Might be raw bytes as array - pre-allocate
            let mut bytes = Vec::with_capacity(arr.len().min(16));
            for v in arr.iter() {
                if let Value::Integer(i) = v {
                    bytes.push(*i as u8);
                }
            }
            validate_trace_id(&bytes)
        }
        _ => Vec::new(),
    }
}

/// Extract span_id with validation.
#[inline]
fn extract_span_id_safe(log: &LogEvent) -> Vec<u8> {
    match log.get(SPAN_ID_KEY) {
        Some(Value::Bytes(b)) => {
            // Optimization: check if already valid 8-byte binary
            if b.len() == 8 {
                return b.to_vec();
            }
            // Otherwise treat as hex string
            // Try direct str conversion if ASCII (common case)
            let s = if b.is_ascii() {
                // Safety: we just checked it's ASCII
                unsafe { std::str::from_utf8_unchecked(b) }
            } else {
                return Vec::new(); // Invalid hex
            };
            from_hex(s)
        }
        Some(Value::Array(arr)) => {
            let mut bytes = Vec::with_capacity(arr.len().min(8));
            for v in arr.iter() {
                if let Value::Integer(i) = v {
                    bytes.push(*i as u8);
                }
            }
            validate_span_id(&bytes)
        }
        _ => Vec::new(),
    }
}

/// Extract instrumentation scope.
fn extract_instrumentation_scope_safe(log: &LogEvent) -> Option<InstrumentationScope> {
    // Extract scope fields using dot-notation string paths
    let scope_name = log
        .get("scope.name")
        .and_then(|v| v.as_bytes())
        .map(|b| String::from_utf8_lossy(b).into_owned());

    let scope_version = log
        .get("scope.version")
        .and_then(|v| v.as_bytes())
        .map(|b| String::from_utf8_lossy(b).into_owned());

    let scope_attrs = log
        .get("scope.attributes")
        .and_then(|v| v.as_object().cloned())
        .map(value_object_to_kv_list)
        .unwrap_or_default();

    if scope_name.is_some() || scope_version.is_some() || !scope_attrs.is_empty() {
        Some(InstrumentationScope {
            name: scope_name.unwrap_or_default(),
            version: scope_version.unwrap_or_default(),
            attributes: scope_attrs,
            dropped_attributes_count: 0,
        })
    } else {
        None
    }
}

/// Extract resource.
#[inline]
fn extract_resource_safe(log: &LogEvent) -> Option<Resource> {
    // Check multiple path patterns (static to avoid allocations)
    const RESOURCE_FIELDS: [&str; 3] = ["resources", "resource", "resource_attributes"];

    for field in RESOURCE_FIELDS {
        if let Some(v) = log.get(field) {
            let attrs = match v {
                Value::Object(obj) => {
                    // Pre-allocate and avoid clone
                    let mut result = Vec::with_capacity(obj.len());
                    for (k, v) in obj.iter() {
                        if matches!(v, Value::Null) {
                            continue;
                        }
                        result.push(KeyValue {
                            key: k.to_string(),
                            value: Some(AnyValue {
                                value: Some(v.clone().into()),
                            }),
                        });
                    }
                    result
                }
                Value::Array(arr) => {
                    // Pre-formatted KeyValue array
                    let mut result = Vec::with_capacity(arr.len());
                    for item in arr.iter() {
                        if let Value::Object(obj) = item {
                            if let Some(key) = obj.get("key").and_then(|v| v.as_str()) {
                                result.push(KeyValue {
                                    key: key.to_string(),
                                    value: obj.get("value").map(|v| AnyValue {
                                        value: Some(v.clone().into()),
                                    }),
                                });
                            }
                        }
                    }
                    result
                }
                _ => continue,
            };

            if !attrs.is_empty() {
                return Some(Resource {
                    attributes: attrs,
                    dropped_attributes_count: 0,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod native_conversion_tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_empty_log_produces_valid_otlp() {
        let log = LogEvent::default();

        // Should not panic, should produce valid (empty) OTLP
        let request = native_log_to_otlp_request(&log);

        assert_eq!(request.resource_logs.len(), 1);
        assert_eq!(request.resource_logs[0].scope_logs.len(), 1);
        assert_eq!(request.resource_logs[0].scope_logs[0].log_records.len(), 1);
    }

    #[test]
    fn test_basic_native_log() {
        let mut log = LogEvent::default();
        log.insert("message", "Test message");
        log.insert("severity_text", "INFO");
        log.insert("severity_number", 9i64);

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.severity_text, "INFO");
        assert_eq!(lr.severity_number, 9);
        assert!(lr.body.is_some());
    }

    #[test]
    fn test_timestamp_as_seconds() {
        let mut log = LogEvent::default();
        log.insert("timestamp", 1704067200i64); // 2024-01-01 00:00:00 UTC in seconds

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        // Should convert to nanoseconds
        assert_eq!(lr.time_unix_nano, 1704067200_000_000_000u64);
    }

    #[test]
    fn test_timestamp_as_nanos() {
        let mut log = LogEvent::default();
        log.insert("timestamp", 1704067200_000_000_000i64); // Already in nanos

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.time_unix_nano, 1704067200_000_000_000u64);
    }

    #[test]
    fn test_timestamp_as_chrono() {
        let mut log = LogEvent::default();
        let ts = Utc::now();
        log.insert("timestamp", ts);

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert!(lr.time_unix_nano > 0);
    }

    #[test]
    fn test_negative_timestamp_handled() {
        let mut log = LogEvent::default();
        log.insert("timestamp", -1i64);

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.time_unix_nano, 0); // Should default to 0
    }

    #[test]
    fn test_severity_number_out_of_range() {
        let mut log = LogEvent::default();
        log.insert("severity_number", 100i64);

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.severity_number, 24); // Clamped to max
    }

    #[test]
    fn test_severity_inferred_from_text() {
        let mut log = LogEvent::default();
        log.insert("severity_text", "ERROR");
        // No severity_number set

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.severity_number, SeverityNumber::Error as i32);
    }

    #[test]
    fn test_message_from_alternative_fields() {
        // Test .msg field
        let mut log = LogEvent::default();
        log.insert("msg", "From msg field");

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert!(lr.body.is_some());
    }

    #[test]
    fn test_attributes_object() {
        let mut log = LogEvent::default();
        log.insert("attributes.key1", "value1");
        log.insert("attributes.key2", 42i64);

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.attributes.len(), 2);
    }

    #[test]
    fn test_trace_id_hex_string() {
        let mut log = LogEvent::default();
        log.insert("trace_id", "0123456789abcdef0123456789abcdef");

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.trace_id.len(), 16);
    }

    #[test]
    fn test_span_id_hex_string() {
        let mut log = LogEvent::default();
        log.insert("span_id", "0123456789abcdef");

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        assert_eq!(lr.span_id.len(), 8);
    }

    #[test]
    fn test_invalid_trace_id() {
        let mut log = LogEvent::default();
        log.insert("trace_id", "not-hex");

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        // Invalid should result in empty
        assert!(lr.trace_id.is_empty());
    }

    #[test]
    fn test_resource_attributes() {
        let mut log = LogEvent::default();
        log.insert("resources.service.name", "test-service");
        log.insert("resources.host.name", "test-host");

        let request = native_log_to_otlp_request(&log);
        let resource = request.resource_logs[0].resource.as_ref().unwrap();

        assert_eq!(resource.attributes.len(), 2);
    }

    #[test]
    fn test_scope() {
        let mut log = LogEvent::default();
        log.insert("scope.name", "test-scope");
        log.insert("scope.version", "1.0.0");

        let request = native_log_to_otlp_request(&log);
        let scope = request.resource_logs[0].scope_logs[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "test-scope");
        assert_eq!(scope.version, "1.0.0");
    }

    #[test]
    fn test_mixed_valid_invalid_fields() {
        let mut log = LogEvent::default();
        log.insert("message", "Valid message");
        log.insert("timestamp", -999i64); // Invalid
        log.insert("severity_number", 9i64); // Valid
        log.insert("trace_id", "not-hex"); // Invalid
        log.insert("attributes.valid", "value"); // Valid

        let request = native_log_to_otlp_request(&log);
        let lr = &request.resource_logs[0].scope_logs[0].log_records[0];

        // Valid fields should be present
        assert!(lr.body.is_some());
        assert_eq!(lr.severity_number, 9);
        assert!(!lr.attributes.is_empty());

        // Invalid fields should have safe defaults
        assert_eq!(lr.time_unix_nano, 0);
        assert!(lr.trace_id.is_empty());
    }
}

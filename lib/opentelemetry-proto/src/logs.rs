use bytes::Bytes;
use chrono::{DateTime, TimeZone, Utc};
use vector_core::{
    config::{LegacyKey, LogNamespace, log_schema},
    event::{Event, LogEvent},
};
use vrl::{core::Value, path};

use super::common::{kv_list_into_value, to_hex};
use crate::proto::{
    common::v1::{InstrumentationScope, any_value::Value as PBValue},
    logs::v1::{LogRecord, ResourceLogs, SeverityNumber},
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
pub const SCHEMA_URL_KEY: &str = "schema_url";
const RESOURCE_DROPPED_ATTRIBUTES_COUNT_KEY: &str = "resource_dropped_attributes_count";

impl ResourceLogs {
    pub fn into_event_iter(self, log_namespace: LogNamespace) -> impl Iterator<Item = Event> {
        let now = Utc::now();
        let resource_schema_url = self.schema_url;

        self.scope_logs.into_iter().flat_map(move |scope_log| {
            let scope = scope_log.scope;
            let scope_schema_url = scope_log.schema_url;
            let resource = self.resource.clone();
            let resource_schema_url = resource_schema_url.clone();
            scope_log.log_records.into_iter().map(move |log_record| {
                ResourceLog {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    log_record,
                    scope_schema_url: scope_schema_url.clone(),
                    resource_schema_url: resource_schema_url.clone(),
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
    scope_schema_url: String,
    resource_schema_url: String,
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

        // Scope-level schema_url (from ScopeLogs)
        if !self.scope_schema_url.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SCOPE_KEY, SCHEMA_URL_KEY))),
                path!(SCOPE_KEY, SCHEMA_URL_KEY),
                self.scope_schema_url,
            );
        }

        // Optional fields
        if let Some(resource) = self.resource {
            if !resource.attributes.is_empty() {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(RESOURCE_KEY))),
                    path!(RESOURCE_KEY),
                    kv_list_into_value(resource.attributes),
                );
            }
            // Resource dropped_attributes_count: only written when > 0 (optional metadata).
            // Legacy namespace: stored at root as "resource_dropped_attributes_count".
            // Vector namespace: stored as "resource_dropped_attributes_count" (separate from
            // "resources" to avoid colliding with user-supplied resource attributes).
            if resource.dropped_attributes_count > 0 {
                log_namespace.insert_source_metadata(
                    SOURCE_NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!(
                        RESOURCE_DROPPED_ATTRIBUTES_COUNT_KEY
                    ))),
                    path!("resource_dropped_attributes_count"),
                    resource.dropped_attributes_count,
                );
            }
        }
        // Resource-level schema_url (from ResourceLogs).
        // Legacy namespace: stored at root as "schema_url".
        // Vector namespace: stored as "resource_schema_url" (separate from "resources" to
        // avoid colliding with user-supplied resource attributes that may use the same key).
        if !self.resource_schema_url.is_empty() {
            log_namespace.insert_source_metadata(
                SOURCE_NAME,
                &mut log,
                Some(LegacyKey::Overwrite(path!(SCHEMA_URL_KEY))),
                path!("resource_schema_url"),
                self.resource_schema_url,
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

        // Log-record level dropped_attributes_count: always written (including zero) because it
        // is a required field per the OTLP LogRecord spec and CUE schema (required: true).
        // This differs from scope/resource dropped counts which are optional metadata.
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(DROPPED_ATTRIBUTES_COUNT_KEY))),
            path!(DROPPED_ATTRIBUTES_COUNT_KEY),
            self.log_record.dropped_attributes_count,
        );

        // According to log data model spec, if observed_time_unix_nano is missing, the collector
        // should set it to the current time. Using DateTime<Utc> (Copy) avoids a Value clone.
        let observed_ts = if self.log_record.observed_time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.observed_time_unix_nano as i64)
        } else {
            now
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(OBSERVED_TIMESTAMP_KEY))),
            path!(OBSERVED_TIMESTAMP_KEY),
            Value::Timestamp(observed_ts),
        );

        // If time_unix_nano is not present (0 represents missing or unknown timestamp) use observed time
        let timestamp = if self.log_record.time_unix_nano > 0 {
            Utc.timestamp_nanos(self.log_record.time_unix_nano as i64)
        } else {
            observed_ts
        };
        log_namespace.insert_source_metadata(
            SOURCE_NAME,
            &mut log,
            log_schema().timestamp_key().map(LegacyKey::Overwrite),
            path!("timestamp"),
            Value::Timestamp(timestamp),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{
        common::v1::{AnyValue, KeyValue, any_value::Value as PBValue},
        logs::v1::ScopeLogs,
    };

    fn make_scope(name: &str, version: &str) -> InstrumentationScope {
        InstrumentationScope {
            name: name.to_string(),
            version: version.to_string(),
            attributes: vec![],
            dropped_attributes_count: 0,
        }
    }

    fn make_resource_logs(
        resource_attrs: Vec<KeyValue>,
        resource_dropped: u32,
        scope: Option<InstrumentationScope>,
        scope_schema_url: &str,
        resource_schema_url: &str,
        log_record: LogRecord,
    ) -> ResourceLogs {
        ResourceLogs {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: resource_dropped,
            }),
            scope_logs: vec![ScopeLogs {
                scope,
                log_records: vec![log_record],
                schema_url: scope_schema_url.to_string(),
            }],
            schema_url: resource_schema_url.to_string(),
        }
    }

    fn make_kv(key: &str, val: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::StringValue(val.to_string())),
            }),
        }
    }

    fn default_log_record() -> LogRecord {
        LogRecord {
            time_unix_nano: 1_000_000_000,
            observed_time_unix_nano: 1_000_000_000,
            severity_number: SeverityNumber::Info as i32,
            severity_text: "INFO".to_string(),
            body: Some(AnyValue {
                value: Some(PBValue::StringValue("test".to_string())),
            }),
            attributes: vec![],
            dropped_attributes_count: 0,
            flags: 0,
            trace_id: vec![],
            span_id: vec![],
        }
    }

    // ========================================================================
    // Tests for schema_url decode (Legacy namespace)
    // ========================================================================

    #[test]
    fn test_scope_schema_url_decoded_legacy() {
        let rl = make_resource_logs(
            vec![],
            0,
            None,
            "https://opentelemetry.io/schemas/1.21.0",
            "",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(
            log.get("scope.schema_url").unwrap().to_string_lossy(),
            "https://opentelemetry.io/schemas/1.21.0"
        );
    }

    #[test]
    fn test_resource_schema_url_decoded_legacy() {
        let rl = make_resource_logs(
            vec![],
            0,
            None,
            "",
            "https://opentelemetry.io/schemas/1.20.0",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert_eq!(
            log.get("schema_url").unwrap().to_string_lossy(),
            "https://opentelemetry.io/schemas/1.20.0"
        );
    }

    #[test]
    fn test_both_schema_urls_decoded_legacy() {
        let rl = make_resource_logs(
            vec![],
            0,
            None,
            "https://scope.schema",
            "https://resource.schema",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();
        assert_eq!(
            log.get("scope.schema_url").unwrap().to_string_lossy(),
            "https://scope.schema"
        );
        assert_eq!(
            log.get("schema_url").unwrap().to_string_lossy(),
            "https://resource.schema"
        );
    }

    #[test]
    fn test_empty_schema_urls_not_inserted() {
        let rl = make_resource_logs(vec![], 0, None, "", "", default_log_record());
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();
        assert!(log.get("scope.schema_url").is_none());
        assert!(log.get("schema_url").is_none());
    }

    // ========================================================================
    // Tests for schema_url decode (Vector namespace)
    // ========================================================================

    #[test]
    fn test_scope_schema_url_decoded_vector() {
        let rl = make_resource_logs(
            vec![],
            0,
            None,
            "https://scope.schema",
            "",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Vector).collect();
        let log = events[0].as_log();
        let metadata = log.metadata().value();
        let scope_schema = metadata
            .get("opentelemetry")
            .and_then(|v| v.get("scope"))
            .and_then(|v| v.get("schema_url"));
        assert!(scope_schema.is_some());
        assert_eq!(
            scope_schema.unwrap().to_string_lossy(),
            "https://scope.schema"
        );
    }

    #[test]
    fn test_resource_schema_url_decoded_vector() {
        let rl = make_resource_logs(
            vec![],
            0,
            None,
            "",
            "https://resource.schema",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Vector).collect();
        let log = events[0].as_log();
        let metadata = log.metadata().value();
        let res_schema = metadata
            .get("opentelemetry")
            .and_then(|v| v.get("resource_schema_url"));
        assert!(res_schema.is_some());
        assert_eq!(
            res_schema.unwrap().to_string_lossy(),
            "https://resource.schema"
        );
    }

    // ========================================================================
    // Tests for resource.dropped_attributes_count
    // ========================================================================

    #[test]
    fn test_resource_dropped_attributes_count_legacy() {
        let rl = make_resource_logs(
            vec![make_kv("service.name", "test")],
            5,
            None,
            "",
            "",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();
        assert_eq!(
            *log.get("resource_dropped_attributes_count").unwrap(),
            Value::Integer(5)
        );
    }

    #[test]
    fn test_resource_dropped_attributes_count_zero_not_inserted() {
        let rl = make_resource_logs(
            vec![make_kv("service.name", "test")],
            0,
            None,
            "",
            "",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();
        assert!(log.get("resource_dropped_attributes_count").is_none());
    }

    #[test]
    fn test_resource_dropped_attributes_count_vector() {
        let rl = make_resource_logs(
            vec![make_kv("service.name", "test")],
            3,
            None,
            "",
            "",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Vector).collect();
        let log = events[0].as_log();
        let metadata = log.metadata().value();
        let dropped = metadata
            .get("opentelemetry")
            .and_then(|v| v.get("resource_dropped_attributes_count"));
        assert!(dropped.is_some());
        assert_eq!(*dropped.unwrap(), Value::Integer(3));
    }

    // ========================================================================
    // Tests for scope fields (verify existing behavior still works)
    // ========================================================================

    #[test]
    fn test_scope_name_version_decoded() {
        let scope = make_scope("my-library", "1.2.3");
        let rl = make_resource_logs(vec![], 0, Some(scope), "", "", default_log_record());
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();
        assert_eq!(
            log.get("scope.name").unwrap().to_string_lossy(),
            "my-library"
        );
        assert_eq!(log.get("scope.version").unwrap().to_string_lossy(), "1.2.3");
    }

    //
    // Combined: all new fields populated together
    //

    #[test]
    fn test_all_new_fields_together() {
        let scope = InstrumentationScope {
            name: "otel-sdk".to_string(),
            version: "2.0.0".to_string(),
            attributes: vec![make_kv("lib.lang", "rust")],
            dropped_attributes_count: 1,
        };
        let rl = make_resource_logs(
            vec![make_kv("host.name", "server-1")],
            2,
            Some(scope),
            "https://scope.schema/1.0",
            "https://resource.schema/1.0",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Legacy).collect();
        let log = events[0].as_log();

        // Scope fields
        assert_eq!(log.get("scope.name").unwrap().to_string_lossy(), "otel-sdk");
        assert_eq!(log.get("scope.version").unwrap().to_string_lossy(), "2.0.0");

        // Schema URLs
        assert_eq!(
            log.get("scope.schema_url").unwrap().to_string_lossy(),
            "https://scope.schema/1.0"
        );
        assert_eq!(
            log.get("schema_url").unwrap().to_string_lossy(),
            "https://resource.schema/1.0"
        );

        // Resource dropped attributes count
        assert_eq!(
            *log.get("resource_dropped_attributes_count").unwrap(),
            Value::Integer(2)
        );

        // Resource attributes still work
        assert!(log.get("resources").is_some());
    }

    #[test]
    fn test_all_new_fields_together_vector_namespace() {
        let scope = InstrumentationScope {
            name: "otel-sdk".to_string(),
            version: "2.0.0".to_string(),
            attributes: vec![make_kv("lib.lang", "rust")],
            dropped_attributes_count: 1,
        };
        let rl = make_resource_logs(
            vec![make_kv("host.name", "server-1")],
            2,
            Some(scope),
            "https://scope.schema/1.0",
            "https://resource.schema/1.0",
            default_log_record(),
        );
        let events: Vec<Event> = rl.into_event_iter(LogNamespace::Vector).collect();
        let log = events[0].as_log();
        let metadata = log.metadata().value();
        let otel = metadata
            .get("opentelemetry")
            .expect("opentelemetry metadata");

        // Scope fields
        let scope_meta = otel.get("scope").expect("scope metadata");
        assert_eq!(
            scope_meta.get("name").unwrap().to_string_lossy(),
            "otel-sdk"
        );
        assert_eq!(
            scope_meta.get("schema_url").unwrap().to_string_lossy(),
            "https://scope.schema/1.0"
        );

        // Resource schema_url is stored as a flat key (not nested under "resources")
        // to avoid colliding with user-supplied resource attributes.
        assert_eq!(
            otel.get("resource_schema_url").unwrap().to_string_lossy(),
            "https://resource.schema/1.0"
        );

        // Resource dropped attributes count (also flat, not under "resources")
        assert_eq!(
            *otel.get("resource_dropped_attributes_count").unwrap(),
            Value::Integer(2)
        );
    }
}

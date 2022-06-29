use crate::gelf_fields::*;
use crate::internal_events::{
    GelfSerializeFailedInvalidFieldName, GelfSerializeFailedInvalidType,
    GelfSerializeFailedMissingField,
};
use bytes::{BufMut, BytesMut};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    event::LogEvent,
    schema,
};

/// On GELF encoding behavior:
///   Graylog has a relaxed parsing. They are much more lenient than the spec would
///   suggest. We've elected to take a more strict approach to maintain backwards compatability
///   in the event that we need to change the behavior to be more relaxed, so that prior versions
///   of vector will still work.
///   The exception is that if 'Additional fields' are found to be missing an underscore prefix and
///   are otherwise valid field names, we prepend the underscore.

/// Regex for matching valid field names. Must contain only word chars, periods and dashes.
static VALID_FIELD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\w\.\-]*$").unwrap());

static MISSING_FIELD_STR: &str = "LogEvent does not contain required field.";

static INVALID_TYPE_STR: &str = "LogEvent contains a value with an invalid type.";

static INVALID_FIELD_NAME_STR: &str = "LogEvent contains a value with an invalid type.";

/// Config used to build a `GelfSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GelfSerializerConfig;

impl GelfSerializerConfig {
    /// Creates a new `GelfSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `GelfSerializer` from this configuration.
    pub fn build(&self) -> GelfSerializer {
        GelfSerializer::new()
    }

    /// The data type of events that are accepted by `GelfSerializer`.
    pub fn input_type() -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement() -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // JSON, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the GELF format.
/// Spec: https://docs.graylog.org/docs/gelf
#[derive(Debug, Clone)]
pub struct GelfSerializer {
    conformed_log: Option<LogEvent>,
}

impl GelfSerializer {
    /// Creates a new `GelfSerializer`.
    pub fn new() -> Self {
        GelfSerializer {
            conformed_log: None,
        }
    }

    /// Encode event and represent it as JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, serde_json::Error> {
        match event {
            Event::Log(log) => serde_json::to_value(&log),
            Event::Metric(_) | Event::Trace(_) => {
                panic!("GELF Serializer does not support Metric or Trace events.")
            }
        }
    }

    /// Validates that the GELF required fields exist in the event.
    fn validate_required_fields(&mut self, log: &LogEvent) -> vector_core::Result<()> {
        // emits the GelfSerializeFailedMissingField internal event and returns Err
        fn emit_missing_field(field: &str) -> vector_core::Result<()> {
            vector_core::internal_event::emit(GelfSerializeFailedMissingField {
                field,
                message: MISSING_FIELD_STR,
            });
            Err(format!("{}: {}", MISSING_FIELD_STR, field).into())
        }

        if !log.contains(VERSION) {
            emit_missing_field(VERSION)?;
        }
        if !log.contains(HOST) {
            emit_missing_field(HOST)?;
        }

        let message_key = log_schema().message_key();
        if !log.contains(SHORT_MESSAGE) {
            // rename the log_schema().message_key() to SHORT_MESSAGE
            if log.contains(message_key) {
                self.conformed_log = Some(log.clone());
                self.conformed_log
                    .as_mut()
                    .unwrap()
                    .rename_key(message_key, SHORT_MESSAGE);
            } else {
                emit_missing_field(SHORT_MESSAGE)?;
            }
        }
        Ok(())
    }

    /// Validates rules for field names and value types.
    fn validate_field_names_and_values(&mut self, log: &LogEvent) -> vector_core::Result<()> {
        // emits the GelfSerializeFailedInvalidType internal event and returns Err
        fn emit_invalid_type(
            field: &str,
            expected_type: &str,
            actual_type: &str,
        ) -> vector_core::Result<()> {
            vector_core::internal_event::emit(GelfSerializeFailedInvalidType {
                field,
                expected_type,
                actual_type,
                message: INVALID_TYPE_STR,
            });
            Err(format!(
                "{}: field: {} type: {} expected_type: {}",
                INVALID_TYPE_STR, field, actual_type, expected_type
            )
            .into())
        }
        if let Some(event_data) = log.as_map() {
            for (key, value) in event_data {
                // validate string values
                if key == VERSION
                    || key == HOST
                    || key == SHORT_MESSAGE
                    || key == FULL_MESSAGE
                    || key == FACILITY
                    || key == FILE
                {
                    if !value.is_bytes() {
                        emit_invalid_type(key, "UTF-8 string", value.kind_str())?;
                    }
                }
                // validate timestamp value
                else if key == TIMESTAMP {
                    if !(value.is_timestamp() || value.is_integer()) {
                        emit_invalid_type(key, "timestamp or integer", value.kind_str())?;
                    }
                }
                // validate integer values
                else if key == LEVEL {
                    if !value.is_integer() {
                        emit_invalid_type(key, "integer", value.kind_str())?;
                    }
                }
                // validate float values
                else if key == LINE {
                    if !(value.is_float() || value.is_integer()) {
                        emit_invalid_type(key, "number", value.kind_str())?;
                    }
                } else {
                    // Additional fields must be prefixed with underscores.
                    // Prepending the underscore since vector adds fields such as 'source_type'
                    // which would otherwise throw errors.
                    if !key.is_empty() && !key.starts_with('_') {
                        if self.conformed_log.is_none() {
                            self.conformed_log = Some(log.clone())
                        }
                        self.conformed_log
                            .as_mut()
                            .unwrap()
                            .rename_key(key.as_str(), &*format!("_{}", &key));
                    }

                    // additional fields must be only word chars, dashes and periods.
                    if !VALID_FIELD.is_match(key) {
                        vector_core::internal_event::emit(GelfSerializeFailedInvalidFieldName {
                            field: key,
                            message: INVALID_FIELD_NAME_STR,
                        });
                        return Err(format!("{}: {}", INVALID_FIELD_NAME_STR, key).into());
                    }
                }
            }
        }
        Ok(())
    }

    /// Determine if input log event is in valid GELF format
    fn validate_event_is_gelf(&mut self, log: &LogEvent) -> vector_core::Result<()> {
        self.validate_required_fields(log)?;
        self.validate_field_names_and_values(log)
    }
}

impl Default for GelfSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder<Event> for GelfSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.as_log();
        let writer = buffer.writer();

        self.validate_event_is_gelf(log)?;

        // if event was conformed to valid GELF, use that instead
        if let Some(conformed) = &self.conformed_log {
            serde_json::to_writer(writer, &conformed)?;
        } else {
            serde_json::to_writer(writer, &log)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::encoding::SerializerConfig;

    use super::*;
    use value::Value;
    use vector_common::btreemap;
    use vector_core::event::{Event, EventMetadata, Metric, MetricKind, MetricValue, TraceEvent};

    fn do_serialize(
        expect_success: bool,
        event_fields: BTreeMap<String, Value>,
    ) -> Option<serde_json::Value> {
        let config = GelfSerializerConfig::new();
        let mut serializer = config.build();
        let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();
        let mut buffer = BytesMut::new();

        if expect_success {
            assert!(serializer.encode(event, &mut buffer).is_ok());
            let buffer_str = std::str::from_utf8(&buffer).unwrap();
            let result = serde_json::from_str(buffer_str);
            assert!(result.is_ok());
            Some(result.unwrap())
        } else {
            assert!(serializer.encode(event, &mut buffer).is_err());
            None
        }
    }

    #[test]
    fn gelf_serde_json_to_value_supported() {
        let serializer = SerializerConfig::Gelf.build().unwrap();

        let log_event = Event::Log(LogEvent::default());
        assert!(serializer.supports_json(&log_event));
        assert!(serializer.to_json_value(log_event).is_ok());
    }

    #[test]
    #[should_panic]
    fn gelf_serde_json_to_value_metric_not_supported() {
        let serializer = SerializerConfig::Gelf.build().unwrap();
        let metric_event = Event::Metric(Metric::new(
            "foo",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        ));
        assert!(!serializer.supports_json(&metric_event));
        serializer.to_json_value(metric_event).unwrap();
    }

    #[test]
    #[should_panic]
    fn gelf_serde_json_to_value_trace_not_supported() {
        let serializer = SerializerConfig::Gelf.build().unwrap();
        let trace_event = Event::Trace(TraceEvent::default());
        assert!(!serializer.supports_json(&trace_event));
        serializer.to_json_value(trace_event).unwrap();
    }

    #[test]
    fn gelf_serializing_valid() {
        let event_fields = btreemap! {
            VERSION => "1.1",
            HOST => "example.org",
            SHORT_MESSAGE => "Some message",
            FULL_MESSAGE => "Even more message",
            FACILITY => "",
            FILE => "/tmp/foobar",
            LINE => Value::Float(ordered_float::NotNan::new(1.5).unwrap()),
            LEVEL => 5,
        };

        let jsn = do_serialize(true, event_fields).unwrap();

        assert_eq!(jsn.get(VERSION).unwrap(), "1.1");
        assert_eq!(jsn.get(HOST).unwrap(), "example.org");
        assert_eq!(jsn.get(SHORT_MESSAGE).unwrap(), "Some message");
    }

    #[test]
    fn gelf_serializing_conformable() {
        // no underscore
        {
            let event_fields = btreemap! {
                VERSION => "1.1",
                HOST => "example.org",
                SHORT_MESSAGE => "Some message",
                "noUnderScore" => 0,
            };

            let jsn = do_serialize(true, event_fields).unwrap();
            assert_eq!(jsn.get("_noUnderScore").unwrap(), 0);
        }

        // "message" => SHORT_MESSAGE
        {
            let event_fields = btreemap! {
                VERSION => "1.1",
                HOST => "example.org",
                log_schema().message_key() => "Some message",
            };

            let jsn = do_serialize(true, event_fields).unwrap();
            assert_eq!(jsn.get(SHORT_MESSAGE).unwrap(), "Some message");
        }
    }

    #[test]
    fn gelf_serializing_invalid_error() {
        // no version
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                SHORT_MESSAGE => "Some message",
            };
            do_serialize(false, event_fields);
        }
        // no host
        {
            let event_fields = btreemap! {
                VERSION => "1.1",
                SHORT_MESSAGE => "Some message",
            };
            do_serialize(false, event_fields);
        }
        // no message
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                VERSION => "1.1",
            };
            do_serialize(false, event_fields);
        }
        // expected string
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                VERSION => "1.1",
                SHORT_MESSAGE => 0,
            };
            do_serialize(false, event_fields);
        }
        // expected integer
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                VERSION => "1.1",
                SHORT_MESSAGE => "Some message",
                LEVEL => "1",
            };
            do_serialize(false, event_fields);
        }
        // expected float
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                VERSION => "1.1",
                SHORT_MESSAGE => "Some message",
                LINE => "1.2",
            };
            do_serialize(false, event_fields);
        }
        // invalid field name
        {
            let event_fields = btreemap! {
                HOST => "example.org",
                VERSION => "1.1",
                SHORT_MESSAGE => "Some message",
                "invalid%field" => "foo",
            };
            do_serialize(false, event_fields);
        }
    }
}

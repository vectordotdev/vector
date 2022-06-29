use crate::gelf_fields::*;
use bytes::{BufMut, BytesMut};
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
    valid_regex: Regex,
    conformed_log: Option<LogEvent>,
}

impl GelfSerializer {
    /// Creates a new `GelfSerializer`.
    pub fn new() -> Self {
        GelfSerializer {
            valid_regex: Regex::new(r"^[\w\.\-]*$").unwrap(),
            conformed_log: None,
        }
    }

    /// Encode event and represent it as JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, serde_json::Error> {
        match event {
            Event::Log(log) => serde_json::to_value(&log),
            // TODO Metric and Trace shouldn't be valid but I was not finding a way to construct a
            // custom serde_json::Error on the fly
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(&trace),
        }
    }

    /// Validates that the GELF required fields exist in the event.
    fn validate_required_fields(&mut self, log: &LogEvent) -> vector_core::Result<()> {
        if !log.contains(VERSION) {
            return Err(format!("LogEvent does not contain required field '{}'", VERSION).into());
        }
        if !log.contains(HOST) {
            return Err(format!("LogEvent does not contain required field '{}'", HOST).into());
        }

        let message_key = log_schema().message_key();
        if log.contains(message_key) {
            self.conformed_log = Some(log.clone());
            self.conformed_log
                .as_mut()
                .unwrap()
                .rename_key(message_key, SHORT_MESSAGE);
        } else if !log.contains(SHORT_MESSAGE) {
            return Err(format!(
                "LogEvent does not contain required field '{}'",
                SHORT_MESSAGE
            )
            .into());
        }
        Ok(())
    }

    /// Validates rules for field names and value types.
    fn validate_field_names_and_values(&mut self, log: &LogEvent) -> vector_core::Result<()> {
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
                        return Err(
                            format!("LogEvent field '{}' should be a UTF-8 string", key).into()
                        );
                    }
                }
                // validate timestamp value
                else if key == TIMESTAMP {
                    if !(value.is_timestamp() || value.is_integer()) {
                        return Err(format!(
                            "LogEvent field '{}' should be a timestamp type or integer",
                            log_schema().timestamp_key()
                        )
                        .into());
                    }
                }
                // validate integer values
                else if key == LEVEL {
                    if !value.is_integer() {
                        return Err(format!("LogEvent field {} should be an integer", key).into());
                    }
                }
                // validate float values
                else if key == LINE {
                    if !(value.is_float() || value.is_integer()) {
                        return Err(format!("LogEvent field '{}' should be a number", key).into());
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
                    if !self.valid_regex.is_match(key) {
                        return Err(format!(
                            "LogEvent field '{}' contains an invalid character",
                            key
                        )
                        .into());
                    }
                }
            }
        }
        Ok(())
    }

    /// Determine if input log event is in valid GELF format. Possible return values:
    ///    - Ok(None)           => The log event is valid GELF
    ///    - Ok(Some(LogEvent)) => The the log event isn't stricly valid GELF, but was
    ///                            conformed to be valid.
    ///    - Err(..)            => The log event is invalid GELF
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

        // if event was conformed during validation, use it instead
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

    use super::*;
    use value::Value;
    use vector_common::btreemap;
    use vector_core::event::{Event, EventMetadata};

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
    fn gelf_serializing_invalid_sanitize() {
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

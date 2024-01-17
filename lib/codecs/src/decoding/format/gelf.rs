use bytes::Bytes;
use chrono::{NaiveDateTime, Utc};
use derivative::Derivative;
use lookup::{event_path, owned_value_path};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::collections::HashMap;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    event::LogEvent,
    schema,
};
use vrl::value::kind::Collection;
use vrl::value::{Kind, Value};

use super::{default_lossy, Deserializer};
use crate::gelf::GELF_TARGET_PATHS;
use crate::{gelf_fields::*, VALID_FIELD_REGEX};

// On GELF decoding behavior:
//   Graylog has a relaxed decoding. They are much more lenient than the spec would
//   suggest. We've elected to take a more strict approach to maintain backwards compatibility
//   in the event that we need to change the behavior to be more relaxed, so that prior versions
//   of vector will still work with the new relaxed decoding.
//
//   Additionally, Graylog's own GELF Output produces GELF messages with any field names present
//   in the sending Stream, exceeding the specified field name character set.

/// Config used to build a `GelfDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct GelfDeserializerConfig {
    /// GELF-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub gelf: GelfDeserializerOptions,
}

impl GelfDeserializerConfig {
    /// Creates a new `GelfDeserializerConfig`.
    pub fn new(options: GelfDeserializerOptions) -> Self {
        Self { gelf: options }
    }

    /// Build the `GelfDeserializer` from this configuration.
    pub fn build(&self) -> GelfDeserializer {
        GelfDeserializer {
            lossy: self.gelf.lossy,
        }
    }

    /// Return the type of event built by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        schema::Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [log_namespace],
        )
        .with_event_field(&owned_value_path!(VERSION), Kind::bytes(), None)
        .with_event_field(&owned_value_path!(HOST), Kind::bytes(), None)
        .with_event_field(&owned_value_path!(SHORT_MESSAGE), Kind::bytes(), None)
        .optional_field(&owned_value_path!(FULL_MESSAGE), Kind::bytes(), None)
        .optional_field(&owned_value_path!(TIMESTAMP), Kind::timestamp(), None)
        .optional_field(&owned_value_path!(LEVEL), Kind::integer(), None)
        .optional_field(&owned_value_path!(FACILITY), Kind::bytes(), None)
        .optional_field(&owned_value_path!(LINE), Kind::integer(), None)
        .optional_field(&owned_value_path!(FILE), Kind::bytes(), None)
        // Every field with an underscore (_) prefix will be treated as an additional field.
        // Allowed characters in field names are any word character (letter, number, underscore), dashes and dots.
        // Libraries SHOULD not allow to send id as additional field ( _id). Graylog server nodes omit this field automatically.
        .unknown_fields(Kind::bytes().or_integer().or_float())
    }
}

/// GELF-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct GelfDeserializerOptions {
    /// Determines whether or not to replace invalid UTF-8 sequences instead of failing.
    ///
    /// When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].
    ///
    /// [U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
    #[serde(
        default = "default_lossy",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    #[derivative(Default(value = "default_lossy()"))]
    pub lossy: bool,
}

/// Deserializer that builds an `Event` from a byte frame containing a GELF log message.
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct GelfDeserializer {
    #[derivative(Default(value = "default_lossy()"))]
    lossy: bool,
}

impl GelfDeserializer {
    /// Create a new `GelfDeserializer`.
    pub fn new(lossy: bool) -> GelfDeserializer {
        GelfDeserializer { lossy }
    }

    /// Builds a LogEvent from the parsed GelfMessage.
    /// The logic follows strictly the documented GELF standard.
    fn message_to_event(&self, parsed: &GelfMessage) -> vector_common::Result<Event> {
        let mut log = LogEvent::from_str_legacy(parsed.short_message.to_string());

        // GELF spec defines the version as 1.1 which has not changed since 2013
        if parsed.version != GELF_VERSION {
            return Err(format!(
                "{} does not match GELF spec version ({})",
                VERSION, GELF_VERSION
            )
            .into());
        }

        log.insert(&GELF_TARGET_PATHS.version, parsed.version.to_string());
        log.insert(&GELF_TARGET_PATHS.host, parsed.host.to_string());

        if let Some(full_message) = &parsed.full_message {
            log.insert(&GELF_TARGET_PATHS.full_message, full_message.to_string());
        }

        if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
            if let Some(timestamp) = parsed.timestamp {
                let naive = NaiveDateTime::from_timestamp_opt(
                    f64::trunc(timestamp) as i64,
                    f64::fract(timestamp) as u32,
                )
                .expect("invalid timestamp");
                log.insert(timestamp_key, naive.and_utc());
                // per GELF spec- add timestamp if not provided
            } else {
                log.insert(timestamp_key, Utc::now());
            }
        }

        if let Some(level) = parsed.level {
            log.insert(&GELF_TARGET_PATHS.level, level);
        }
        if let Some(facility) = &parsed.facility {
            log.insert(&GELF_TARGET_PATHS.facility, facility.to_string());
        }
        if let Some(line) = parsed.line {
            log.insert(
                &GELF_TARGET_PATHS.line,
                Value::Float(ordered_float::NotNan::new(line).expect("JSON doesn't allow NaNs")),
            );
        }
        if let Some(file) = &parsed.file {
            log.insert(&GELF_TARGET_PATHS.file, file.to_string());
        }

        if let Some(add) = &parsed.additional_fields {
            for (key, val) in add.iter() {
                // per GELF spec, filter out _id
                if key == "_id" {
                    continue;
                }
                // per GELF spec, Additional field names must be prefixed with an underscore
                if !key.starts_with('_') {
                    return Err(format!(
                        "'{}' field is invalid. \
                                       Additional field names must be prefixed with an underscore.",
                        key
                    )
                    .into());
                }
                // per GELF spec, Additional field names must be characters dashes or dots
                if !VALID_FIELD_REGEX.is_match(key) {
                    return Err(format!("'{}' field contains invalid characters. Field names may \
                                       contain only letters, numbers, underscores, dashes and dots.", key).into());
                }

                // per GELF spec, Additional field values must be either strings or numbers
                if val.is_string() || val.is_number() {
                    let vector_val: Value = val.into();
                    log.insert(event_path!(key.as_str()), vector_val);
                } else {
                    let type_ = match val {
                        serde_json::Value::Null => "null",
                        serde_json::Value::Bool(_) => "boolean",
                        serde_json::Value::Number(_) => "number",
                        serde_json::Value::String(_) => "string",
                        serde_json::Value::Array(_) => "array",
                        serde_json::Value::Object(_) => "object",
                    };
                    return Err(format!("The value type for field {} is an invalid type ({}). Additional field values \
                                       should be either strings or numbers.", key, type_).into());
                }
            }
        }
        Ok(Event::Log(log))
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct GelfMessage {
    version: String,
    host: String,
    short_message: String,
    full_message: Option<String>,
    timestamp: Option<f64>,
    level: Option<u8>,
    facility: Option<String>,
    line: Option<f64>,
    file: Option<String>,
    #[serde(flatten)]
    additional_fields: Option<HashMap<String, serde_json::Value>>,
}

impl Deserializer for GelfDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        _log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let parsed: GelfMessage = match self.lossy {
            true => serde_json::from_str(&String::from_utf8_lossy(&bytes)),
            false => serde_json::from_slice(&bytes),
        }?;
        let event = self.message_to_event(&parsed)?;

        Ok(smallvec![event])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use chrono::NaiveDateTime;
    use lookup::event_path;
    use serde_json::json;
    use similar_asserts::assert_eq;
    use smallvec::SmallVec;
    use vector_core::{config::log_schema, event::Event};
    use vrl::value::Value;

    fn deserialize_gelf_input(
        input: &serde_json::Value,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let config = GelfDeserializerConfig::default();
        let deserializer = config.build();
        let buffer = Bytes::from(serde_json::to_vec(&input).unwrap());
        deserializer.parse(buffer, LogNamespace::Legacy)
    }

    /// Validates all the spec'd fields of GELF are deserialized correctly.
    #[test]
    fn gelf_deserialize_correctness() {
        let add_on_int_in = "_an.add-field_int";
        let add_on_str_in = "_an.add-field_str";

        let input = json!({
            VERSION: "1.1",
            HOST: "example.org",
            SHORT_MESSAGE: "A short message that helps you identify what is going on",
            FULL_MESSAGE: "Backtrace here\n\nmore stuff",
            TIMESTAMP: 1385053862.3072,
            LEVEL: 1,
            FACILITY: "foo",
            LINE: 42,
            FILE: "/tmp/bar",
            add_on_int_in: 2001.1002,
            add_on_str_in: "A Space Odyssey",
        });

        // Ensure that we can parse the gelf json successfully
        let events = deserialize_gelf_input(&input).unwrap();
        assert_eq!(events.len(), 1);

        let log = events[0].as_log();

        assert_eq!(
            log.get(VERSION),
            Some(&Value::Bytes(Bytes::from_static(b"1.1")))
        );
        assert_eq!(
            log.get(HOST),
            Some(&Value::Bytes(Bytes::from_static(b"example.org")))
        );
        assert_eq!(
            log.get(log_schema().message_key_target_path().unwrap()),
            Some(&Value::Bytes(Bytes::from_static(
                b"A short message that helps you identify what is going on"
            )))
        );
        assert_eq!(
            log.get(FULL_MESSAGE),
            Some(&Value::Bytes(Bytes::from_static(
                b"Backtrace here\n\nmore stuff"
            )))
        );
        // Vector does not use the nanos
        let naive = NaiveDateTime::from_timestamp_opt(1385053862, 0).expect("invalid timestamp");
        assert_eq!(log.get(TIMESTAMP), Some(&Value::Timestamp(naive.and_utc())));
        assert_eq!(log.get(LEVEL), Some(&Value::Integer(1)));
        assert_eq!(
            log.get(FACILITY),
            Some(&Value::Bytes(Bytes::from_static(b"foo")))
        );
        assert_eq!(
            log.get(LINE),
            Some(&Value::Float(ordered_float::NotNan::new(42.0).unwrap()))
        );
        assert_eq!(
            log.get(FILE),
            Some(&Value::Bytes(Bytes::from_static(b"/tmp/bar")))
        );
        assert_eq!(
            log.get(event_path!(add_on_int_in)),
            Some(&Value::Float(
                ordered_float::NotNan::new(2001.1002).unwrap()
            ))
        );
        assert_eq!(
            log.get(event_path!(add_on_str_in)),
            Some(&Value::Bytes(Bytes::from_static(b"A Space Odyssey")))
        );
    }

    /// Validates deserialization succeeds for edge case inputs.
    #[test]
    fn gelf_deserializing_edge_cases() {
        // timestamp is set if omitted from input
        {
            let input = json!({
                HOST: "example.org",
                SHORT_MESSAGE: "foobar",
                VERSION: "1.1",
            });
            let events = deserialize_gelf_input(&input).unwrap();
            assert_eq!(events.len(), 1);
            let log = events[0].as_log();
            assert!(log.contains(log_schema().message_key_target_path().unwrap()));
        }

        // filter out id
        {
            let input = json!({
                HOST: "example.org",
                SHORT_MESSAGE: "foobar",
                VERSION: "1.1",
                "_id": "S3creTz",
            });
            let events = deserialize_gelf_input(&input).unwrap();
            assert_eq!(events.len(), 1);
            let log = events[0].as_log();
            assert!(!log.contains(event_path!("_id")));
        }
    }

    /// Validates the error conditions in deserialization
    #[test]
    fn gelf_deserializing_err() {
        fn validate_err(input: &serde_json::Value) {
            assert!(deserialize_gelf_input(input).is_err());
        }
        //  invalid character in field name
        validate_err(&json!({
            HOST: "example.org",
            SHORT_MESSAGE: "foobar",
            VERSION: "1.1",
            "_bad%key": "raboof",
        }));

        //  not prefixed with underscore
        validate_err(&json!({
            HOST: "example.org",
            SHORT_MESSAGE: "foobar",
            VERSION: "1.1",
            "bad-key": "raboof",
        }));

        // missing short_message
        validate_err(&json!({
            HOST: "example.org",
            VERSION: "1.1",
        }));

        // host is not specified
        validate_err(&json!({
            SHORT_MESSAGE: "foobar",
            VERSION: "1.1",
        }));

        // host is not a string
        validate_err(&json!({
            HOST: 42,
            SHORT_MESSAGE: "foobar",
            VERSION: "1.1",
        }));

        //  level / line is string and not numeric
        validate_err(&json!({
            HOST: "example.org",
            VERSION: "1.1",
            SHORT_MESSAGE: "foobar",
            LEVEL: "baz",
        }));
    }
}

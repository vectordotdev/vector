use bytes::Bytes;
use chrono::Utc;
use derivative::Derivative;
use serde_json::value::RawValue;
use smallvec::{SmallVec, smallvec};
use vector_config::configurable_component;
use vector_core::{
    config::{DataType, LogNamespace, log_schema},
    event::{Event, LogEvent},
    schema,
};
use vrl::core::Value;
use vrl::value::Kind;

use super::{Deserializer, default_lossy};

/// Controls how JSON floating-point numbers are parsed.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ParseFloat {
    /// Parse floating-point numbers as IEEE 754 `f64` values (default).
    ///
    /// This may lose precision for very large integers or decimals with many significant digits.
    #[default]
    Float,

    /// Parse floating-point numbers as exact decimal values using `rust_decimal::Decimal`.
    ///
    /// This preserves the exact string representation from the JSON source,
    /// supporting up to 28-29 significant digits without rounding errors.
    #[configurable(metadata(status = "beta"))]
    Decimal,
}

/// Config used to build a `JsonDeserializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct JsonDeserializerConfig {
    /// JSON-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub json: JsonDeserializerOptions,
}

impl JsonDeserializerConfig {
    /// Creates a new `JsonDeserializerConfig`.
    pub fn new(options: JsonDeserializerOptions) -> Self {
        Self { json: options }
    }

    /// Build the `JsonDeserializer` from this configuration.
    pub fn build(&self) -> JsonDeserializer {
        Into::<JsonDeserializer>::into(self)
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                let mut definition =
                    schema::Definition::empty_legacy_namespace().unknown_fields(Kind::json());

                if let Some(timestamp_key) = log_schema().timestamp_key() {
                    definition = definition.try_with_field(
                        timestamp_key,
                        // The JSON decoder will try to insert a new `timestamp`-type value into the
                        // "timestamp_key" field, but only if that field doesn't already exist.
                        Kind::json().or_timestamp(),
                        Some("timestamp"),
                    );
                }
                definition
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(Kind::json(), [log_namespace])
            }
        }
    }
}

/// JSON-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct JsonDeserializerOptions {
    /// Determines whether to replace invalid UTF-8 sequences instead of failing.
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

    /// Controls how JSON floating-point numbers are parsed.
    ///
    /// Accepted values: `"float"` (default) or `"decimal"`.
    ///
    /// When set to `"decimal"`, non-integer numbers are parsed as exact decimal values
    /// (using `rust_decimal::Decimal`), preserving precision for values like
    /// `12345678901234567890.123`.
    ///
    /// When set to `"float"` (default), numbers are parsed as i64 or f64, which may lose
    /// precision for very large integers or decimals with many significant digits.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub parse_float: ParseFloat,
}

/// Deserializer that builds `Event`s from a byte frame containing JSON.
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct JsonDeserializer {
    #[derivative(Default(value = "default_lossy()"))]
    lossy: bool,

    parse_float: ParseFloat,
}

impl JsonDeserializer {
    /// Creates a new `JsonDeserializer`.
    pub fn new(lossy: bool, parse_float: ParseFloat) -> Self {
        Self { lossy, parse_float }
    }

    /// Parse bytes as JSON, handling lossy UTF-8 conversion if configured.
    fn parse_json_bytes<T: serde::de::DeserializeOwned>(
        &self,
        bytes: &[u8],
    ) -> vector_common::Result<T> {
        if self.lossy {
            let s = String::from_utf8_lossy(bytes);
            serde_json::from_str(&s)
        } else {
            serde_json::from_slice(bytes)
        }
        .map_err(|e| format!("Error parsing JSON: {e:?}").into())
    }
}

impl Deserializer for JsonDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        // It's common to receive empty frames when parsing NDJSON, since it
        // allows multiple empty newlines. We proceed without a warning here.
        if bytes.is_empty() {
            return Ok(smallvec![]);
        }

        let value = if self.parse_float == ParseFloat::Decimal {
            let raw_json: Box<RawValue> = self.parse_json_bytes(&bytes)?;
            Value::try_from(raw_json.as_ref())
                .map_err(|error| format!("Error parsing JSON: {error:?}"))?
        } else {
            let json: serde_json::Value = self.parse_json_bytes(&bytes)?;
            Value::from(json)
        };

        let values = match value {
            Value::Array(values) => values,
            other => vec![other],
        };

        let events = match log_namespace {
            LogNamespace::Vector => values
                .into_iter()
                .map(|value| Event::Log(LogEvent::from(value)))
                .collect(),
            LogNamespace::Legacy => {
                let mut events = values
                    .into_iter()
                    .map(|value| match value {
                        Value::Object(fields) => Ok(Event::Log(LogEvent::from(fields))),
                        _ => Err("Attempted to convert non-Object JSON into an Event.".into()),
                    })
                    .collect::<vector_common::Result<SmallVec<[Event; 1]>>>()?;

                let timestamp = Utc::now();
                if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
                    for event in &mut events {
                        let log = event.as_mut_log();
                        if !log.contains(timestamp_key) {
                            log.insert(timestamp_key, timestamp);
                        }
                    }
                }

                events
            }
        };

        Ok(events)
    }
}

impl From<&JsonDeserializerConfig> for JsonDeserializer {
    fn from(config: &JsonDeserializerConfig) -> Self {
        Self {
            lossy: config.json.lossy,
            parse_float: config.json.parse_float,
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_core::config::log_schema;

    use super::*;

    #[test]
    fn deserialize_json() {
        let input = Bytes::from(r#"{ "foo": 123 }"#);
        let deserializer = JsonDeserializer::default();

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["foo"], 123.into());
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_non_object_vector_namespace() {
        let input = Bytes::from(r#"null"#);
        let deserializer = JsonDeserializer::default();

        let namespace = LogNamespace::Vector;
        let events = deserializer.parse(input.clone(), namespace).unwrap();
        let mut events = events.into_iter();

        let event = events.next().unwrap();
        let log = event.as_log();
        assert_eq!(log["."], Value::Null);

        assert_eq!(events.next(), None);
    }

    #[test]
    fn deserialize_json_array() {
        let input = Bytes::from(r#"[{ "foo": 123 }, { "bar": 456 }]"#);
        let deserializer = JsonDeserializer::default();
        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["foo"], 123.into());
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["bar"], 456.into());
                assert_eq!(
                    log.get(log_schema().timestamp_key_target_path().unwrap())
                        .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_skip_empty() {
        let input = Bytes::from("");
        let deserializer = JsonDeserializer::default();

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            assert!(events.is_empty());
        }
    }

    #[test]
    fn deserialize_error_invalid_json() {
        let input = Bytes::from("{ foo");
        let deserializer = JsonDeserializer::default();

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }

    #[test]
    fn deserialize_lossy_replace_invalid_utf8() {
        let input = Bytes::from(b"{ \"foo\": \"Hello \xF0\x90\x80World\" }".as_slice());
        let deserializer = JsonDeserializer::new(true, ParseFloat::Float);

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let events = deserializer.parse(input.clone(), namespace).unwrap();
            let mut events = events.into_iter();

            {
                let event = events.next().unwrap();
                let log = event.as_log();
                assert_eq!(log["foo"], b"Hello \xEF\xBF\xBDWorld".into());
                assert_eq!(
                    log.get((
                        lookup::PathPrefix::Event,
                        log_schema().timestamp_key().unwrap()
                    ))
                    .is_some(),
                    namespace == LogNamespace::Legacy
                );
            }

            assert_eq!(events.next(), None);
        }
    }

    #[test]
    fn deserialize_non_lossy_error_invalid_utf8() {
        let input = Bytes::from(b"{ \"foo\": \"Hello \xF0\x90\x80World\" }".as_slice());
        let deserializer = JsonDeserializer::new(false, ParseFloat::Float);

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            assert!(deserializer.parse(input.clone(), namespace).is_err());
        }
    }
}

#[cfg(test)]
mod decimal_precision_tests {
    use super::*;

    #[test]
    fn preserves_high_precision_decimal() {
        let input = Bytes::from(r#"{"val": 12345678901234567890.123}"#);
        let deser = JsonDeserializer::new(false, ParseFloat::Decimal);
        let events = deser.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        let val = log.get("val").unwrap();
        assert!(val.is_decimal(), "Expected Decimal, got {:?}", val);
    }

    #[test]
    fn integers_become_integer() {
        let input = Bytes::from(r#"{"int": 42}"#);
        let deser = JsonDeserializer::new(false, ParseFloat::Decimal);
        let events = deser.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        assert!(matches!(log.get("int"), Some(Value::Integer(42))));
    }

    #[test]
    fn decimals_become_decimal() {
        let input = Bytes::from(r#"{"float": 3.14}"#);
        let deser = JsonDeserializer::new(false, ParseFloat::Decimal);
        let events = deser.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        let val = log.get("float").unwrap();
        assert!(val.is_decimal(), "Expected Decimal, got {:?}", val);
    }

    #[test]
    fn float_mode_uses_standard_conversion() {
        let input = Bytes::from(r#"{"int": 42, "float": 3.14}"#);
        let deser = JsonDeserializer::new(false, ParseFloat::Float);
        let events = deser.parse(input, LogNamespace::Vector).unwrap();
        let log = events[0].as_log();
        let int_val = log.get("int").unwrap();
        let float_val = log.get("float").unwrap();
        assert!(int_val.is_integer(), "Expected Integer, got {:?}", int_val);
        assert!(float_val.is_float(), "Expected Float, got {:?}", float_val);
    }
}

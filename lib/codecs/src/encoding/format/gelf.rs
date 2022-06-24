use crate::gelf_fields::*;
use bytes::{BufMut, BytesMut};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use value::Value;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    event::LogEvent,
    schema,
};

/// Options for building a `GelfSerializer`.
#[derive(Debug, Clone, Default, Copy, Deserialize, Serialize)]
pub struct GelfSerializerOptions {
    #[serde(default)]
    sanitize: bool,
}

/// Config used to build a `GelfSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GelfSerializerConfig {
    /// Configuration pptions for the GelfSerializer
    pub options: GelfSerializerOptions,
}

impl GelfSerializerConfig {
    /// Creates a new `GelfSerializerConfig`.
    pub const fn new(options: GelfSerializerOptions) -> Self {
        Self { options }
    }

    /// Build the `GelfSerializer` from this configuration.
    pub fn build(&self) -> GelfSerializer {
        GelfSerializer::new(self.options)
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
    invalid_regex: Regex,
    sanitize: bool,
}

/// Whether the non-conforming event is capable of being conformed to GELF
/// standard by vector or not.
#[derive(Clone)]
enum EventGelfConformity {
    Conformable(String),
    Unconformable(String),
}

impl GelfSerializer {
    /// Creates a new `GelfSerializer`.
    pub fn new(options: GelfSerializerOptions) -> Self {
        GelfSerializer {
            valid_regex: Regex::new(r"^_[\w\.\-]*$").unwrap(),
            invalid_regex: Regex::new(r"[^\w\.\-]+").unwrap(),
            sanitize: options.sanitize,
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

    /// Helper function to either conform the value at 'key' within the conformed log event,
    /// or return the provided error.
    fn conform<F: Fn(&mut Value)>(
        &self,
        clog: &mut Option<LogEvent>,
        conformed: &mut bool,
        key: &str,
        err: Result<(), EventGelfConformity>,
        f: F,
    ) -> Result<(), EventGelfConformity> {
        if let Some(clog) = clog {
            // key is present per caller logic
            let c_val = clog.get_mut(key).unwrap();
            f(c_val);
            *conformed = true;
            Ok(())
        } else {
            err
        }
    }

    /// Return Ok if value is a string. Otherwise, determine if it is possible to conform
    /// the value to a string, and do the conformation if configured to do so.
    fn expect_bytes_value(
        &self,
        key: &str,
        value: &Value,
        clog: &mut Option<LogEvent>,
        conformed: &mut bool,
    ) -> Result<(), EventGelfConformity> {
        if !value.is_bytes() {
            if value.is_float() || value.is_integer() || value.is_boolean() {
                self.conform(
                    clog,
                    conformed,
                    key,
                    Err(EventGelfConformity::Conformable(format!(
                        "LogEvent field {} should be a UTF-8 string",
                        key
                    ))),
                    |c_val| {
                        *c_val = Value::Bytes(value.coerce_to_bytes());
                    },
                )?;
            } else {
                return Err(EventGelfConformity::Unconformable(format!(
                    "LogEvent field {} should be a UTF-8 string",
                    key
                )));
            }
        }
        Ok(())
    }

    /// Return Ok if value is an integer. Otherwise, determine if it is possible to conform
    /// the value to an integer, and do the conformation if configured to do so.
    fn expect_integer_value(
        &self,
        key: &str,
        value: &Value,
        clog: &mut Option<LogEvent>,
        conformed: &mut bool,
    ) -> Result<(), EventGelfConformity> {
        let conformable = EventGelfConformity::Conformable(format!(
            "LogEvent field {} should be an integer",
            key
        ));
        let unconformable = EventGelfConformity::Unconformable(format!(
            "LogEvent field {} should be an integer",
            key
        ));
        if !value.is_integer() {
            // if the value is a string and that string can be parse into an integer
            if value.is_bytes() {
                std::str::from_utf8(value.as_bytes().unwrap())
                    .map(|int_str| {
                        int_str
                            .parse::<i64>()
                            .map(|integer| {
                                self.conform(clog, conformed, key, Err(conformable), |c_val| {
                                    *c_val = Value::Integer(integer);
                                })
                            })
                            .map_err(|_| unconformable.clone())?
                    })
                    .map_err(|_| unconformable)??;
            }
            // round off floats
            else if value.is_float() {
                self.conform(clog, conformed, key, Err(conformable), |c_val| {
                    *c_val = Value::Integer(value.as_float().unwrap().round() as i64);
                })?;
            }
            // false -> 0 , true -> 1
            else if value.is_boolean() {
                self.conform(clog, conformed, key, Err(conformable), |c_val| {
                    *c_val = Value::Integer(value.as_boolean().unwrap() as i64);
                })?;
            } else {
                return Err(EventGelfConformity::Unconformable(format!(
                    "LogEvent field {} should be an integer",
                    key
                )));
            }
        }
        Ok(())
    }

    /// Determine if input log event is in valid GELF format. Possible return values:
    ///    - Ok(None)           => The log event is valid GELF
    ///    - Ok(Some(LogEvent)) => The the log event isn't valid GELF, but was conformed to
    ///                            valid GELF due to the sanitize configuration flag being set.
    ///
    ///    - Err(EventGelfConformity::Conformable)
    ///         => The log event isn't valid GELF, but could be conformed to be valid if the
    ///            user were to set the sanitize configuration option to true.
    ///    - Err(EventGelfConformity::UnConformable)
    ///         => The log event isn't valid GELF and vector is unable to conform it.
    fn is_event_valid_gelf(&self, log: &LogEvent) -> Result<Option<LogEvent>, EventGelfConformity> {
        //
        // TODO the GELF decoder is more relaxed than this, more closely mirroring the behavior of
        // the graylog node. Which means as is, a user could pass in a GELF message to the decoder
        // and it might be missing HOST and VERSION and that would succeed, but encoding it would
        // fail

        // VERSION, HOST and <MESSAGE> are all required fields
        if !log.contains(VERSION) {
            return Err(EventGelfConformity::Unconformable(format!(
                "LogEvent does not contain field {}",
                VERSION
            )));
        }
        if !log.contains(HOST) {
            return Err(EventGelfConformity::Unconformable(format!(
                "LogEvent does not contain field {}",
                HOST
            )));
        }

        let message_key = log_schema().message_key();
        if !log.contains(message_key) {
            return Err(EventGelfConformity::Unconformable(format!(
                "LogEvent does not contain field {}",
                log_schema().message_key()
            )));
        }

        let mut conformed_log = if self.sanitize {
            Some(log.clone())
        } else {
            None
        };

        let mut conformed = false;

        if let Some(event_data) = log.as_map() {
            for (key, value) in event_data {
                // validate string values
                if key == VERSION
                    || key == HOST
                    || key == message_key
                    || key == FULL_MESSAGE
                    || key == FACILITY
                    || key == FILE
                {
                    self.expect_bytes_value(key, value, &mut conformed_log, &mut conformed)?;
                }
                // validate timestamp value
                else if key == TIMESTAMP {
                    if !value.is_timestamp() || value.is_integer() {
                        return Err(EventGelfConformity::Unconformable(format!(
                            "LogEvent field {} should be a timestamp type or integer",
                            log_schema().timestamp_key()
                        )));
                    }
                }
                // validate integer values
                else if key == LEVEL || key == LINE {
                    self.expect_integer_value(key, value, &mut conformed_log, &mut conformed)?;
                } else {
                    // additional fields must be prefixed with underscores
                    // NOTE: electing to conform on this rule even if the sanitize flag is not set
                    // because otherwise vector-added fields (such as "source_type: will throw errors
                    if !key.is_empty() && !key.starts_with('_') && conformed_log.is_none() {
                        let mut clog = log.clone();
                        clog.rename_key(key.as_str(), &*format!("_{}", &key));
                        conformed_log = Some(clog);
                        conformed = true;
                    }

                    // additional fields must be only word chars, dashes and periods.
                    if !self.valid_regex.is_match(key) {
                        // replace offending characters with dashes
                        if let Some(clog) = &mut conformed_log {
                            let new_key = self.invalid_regex.replace_all(key, "-");
                            clog.rename_key(key.as_str(), &*new_key);
                            conformed = true;
                        } else {
                            return Err(EventGelfConformity::Conformable(format!(
                                "LogEvent field {} contains an invalid character",
                                key
                            )));
                        }
                    }
                }
            }
        }
        if conformed {
            Ok(conformed_log)
        } else {
            Ok(None)
        }
    }
}

impl Default for GelfSerializer {
    fn default() -> Self {
        Self::new(GelfSerializerOptions { sanitize: false })
    }
}

impl Encoder<Event> for GelfSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.as_log();
        let writer = buffer.writer();

        match self.is_event_valid_gelf(log) {
            Ok(conformed) => {
                // use conformed log event if it exists, otherwise the original
                if let Some(conformed) = conformed {
                    serde_json::to_writer(writer, &conformed)?;
                } else {
                    serde_json::to_writer(writer, &log)?;
                }
                Ok(())
            }
            Err(conformity) => match conformity {
                EventGelfConformity::Conformable(s) => {
                    Err(format!("Event does not conform to GELF specification but is sanitizable, \
                                try setting the sanitize configuration option to 'true' for the encoder: {}", s).into())
                },
                EventGelfConformity::Unconformable(s) => {
                    if self.sanitize {
                        Err(format!("Event does not conform to GELF specification. Vector was not able to sanitize the event: {}", s).into())
                    } else {
                        Err(format!("Event does not conform to GELF specification: {}", s).into())
                    }
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_common::btreemap;
    use vector_core::event::{Event, EventMetadata};

    /// TODO expand unit tests once confirmed encoding behavior
    #[test]
    fn gelf_serializing_valid() {
        let config = GelfSerializerConfig::default();
        let mut serializer = config.build();

        let event_fields = btreemap! {
            VERSION => "1.1",
            HOST => "example.org",
            log_schema().message_key() => "Some message",
        };
        let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();

        let mut buffer = BytesMut::new();

        assert!(serializer.encode(event, &mut buffer).is_ok());
    }

    #[test]
    fn gelf_serializing_invalid_sanitize() {
        let config = GelfSerializerConfig {
            options: GelfSerializerOptions { sanitize: true },
        };
        let mut serializer = config.build();

        let event_fields = btreemap! {
            VERSION => "1.1",
            HOST => "example.org",
            LINE => "1",
            log_schema().message_key() => "Some message",
        };
        let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();

        let mut buffer = BytesMut::new();

        assert!(serializer.encode(event, &mut buffer).is_ok());
    }

    #[test]
    fn gelf_serializing_invalid_no_sanitize() {
        let config = GelfSerializerConfig::default();
        let mut serializer = config.build();

        let event_fields = btreemap! {
            VERSION => "1.1",
            HOST => "example.org",
            LINE => "1",
            log_schema().message_key() => "Some message",
        };
        let event: Event = LogEvent::from_map(event_fields, EventMetadata::default()).into();

        let mut buffer = BytesMut::new();

        assert!(serializer.encode(event, &mut buffer).is_err());
    }
}

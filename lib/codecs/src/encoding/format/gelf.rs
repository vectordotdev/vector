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
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(&trace),
        }
    }

    // fn massage_gelf_format(&self, log: &mut LogEvent) -> Result<(), EventGelfConformity> {
    //     fn expect_bytes_value(
    //         key: &str,
    //         value: &Value,
    //         log: &mut LogEvent,
    //     ) -> Result<(), EventGelfConformity> {
    //         if !value.is_bytes() {
    //             if value.is_float() || value.is_integer() || value.is_boolean() {
    //                 // TODO modify value
    //             } else {
    //                 return Err(EventGelfConformity::Unconformable(
    //                     format!("LogEvent field {} should be a UTF-8 string", key).into(),
    //                 ));
    //             }
    //         }
    //         Ok(())
    //     }

    //     fn expect_number_value(
    //         key: &str,
    //         value: &Value,
    //         log: &mut LogEvent,
    //     ) -> Result<(), EventGelfConformity> {
    //         if !value.is_integer() {
    //             if value.is_bytes() {
    //                 let v = value.as_bytes().unwrap();
    //                 match std::str::from_utf8(&v) {
    //                     Ok(_) => {
    //                         // TODO modify value
    //                     }
    //                     Err(_) => {
    //                         return Err(EventGelfConformity::Unconformable(
    //                             format!("LogEvent field {} should be an integer", key).into(),
    //                         ))
    //                     }
    //                 }
    //             } else if value.is_float() || value.is_boolean() {
    //                 // TODO modify value
    //             } else {
    //                 return Err(EventGelfConformity::Unconformable(
    //                     format!("LogEvent field {} should be a UTF-8 string", key).into(),
    //                 ));
    //             }
    //         }
    //         Ok(())
    //     }

    //     if let Some(event_data) = log.as_map() {
    //         for (key, value) in event_data {
    //             if key == VERSION {
    //                 expect_bytes_value(&key, value, log)?;
    //             } else if key == HOST {
    //                 expect_bytes_value(&key, value, log)?;
    //             } else if key == log_schema().message_key() {
    //                 expect_bytes_value(&key, value, log)?;
    //             } else if key == FULL_MESSAGE || key == FACILITY || key == FILE {
    //                 expect_bytes_value(&key, value, log)?;
    //             } else if key == TIMESTAMP {
    //                 if !value.is_timestamp() || value.is_integer() {
    //                     return Err(EventGelfConformity::Unconformable(
    //                         format!(
    //                             "LogEvent field {} should be a timestamp type or integer",
    //                             log_schema().timestamp_key()
    //                         )
    //                         .into(),
    //                     ));
    //                 }
    //             } else if key == LEVEL || key == FILE {
    //                 expect_number_value(&key, value, log)?;
    //             } else {
    //                 if key.len() > 0 && key.chars().nth(0).unwrap() != '_' {
    //                     return Err(EventGelfConformity::Conformable(
    //                         format!("LogEvent field {} is not underscore prefixed", key).into(),
    //                     ));
    //                 }
    //                 if !self.regex.is_match(key) {
    //                     return Err(EventGelfConformity::Conformable(
    //                         format!("LogEvent field {} contains an invalid character", key).into(),
    //                     ));
    //                 }
    //             }
    //         }
    //     }

    //     Ok(())
    // }

    // fn is_event_valid_gelf(&self, log: &LogEvent) -> Result<(), EventGelfConformity> {
    //     let mut has_version = false;
    //     let mut has_host = false;
    //     let mut has_message = false;

    //     fn expect_bytes_value(key: &str, value: &Value) -> Result<(), EventGelfConformity> {
    //         if !value.is_bytes() {
    //             if value.is_float() || value.is_integer() || value.is_boolean() {
    //                 return Err(EventGelfConformity::Conformable(
    //                     format!("LogEvent field {} should be a UTF-8 string", key).into(),
    //                 ));
    //             } else {
    //                 return Err(EventGelfConformity::Unconformable(
    //                     format!("LogEvent field {} should be a UTF-8 string", key).into(),
    //                 ));
    //             }
    //         }
    //         Ok(())
    //     }

    //     fn expect_number_value(key: &str, value: &Value) -> Result<(), EventGelfConformity> {
    //         if !value.is_integer() {
    //             if value.is_bytes() {
    //                 let v = value.as_bytes().unwrap();
    //                 match std::str::from_utf8(&v) {
    //                     Ok(_) => {
    //                         return Err(EventGelfConformity::Conformable(
    //                             format!("LogEvent field {} should be an integer", key).into(),
    //                         ))
    //                     }
    //                     Err(_) => {
    //                         return Err(EventGelfConformity::Unconformable(
    //                             format!("LogEvent field {} should be an integer", key).into(),
    //                         ))
    //                     }
    //                 }
    //             } else if value.is_float() || value.is_boolean() {
    //                 return Err(EventGelfConformity::Conformable(
    //                     format!("LogEvent field {} should be an integer", key).into(),
    //                 ));
    //             } else {
    //                 return Err(EventGelfConformity::Unconformable(
    //                     format!("LogEvent field {} should be a UTF-8 string", key).into(),
    //                 ));
    //             }
    //         }
    //         Ok(())
    //     }

    //     if let Some(event_data) = log.as_map() {
    //         for (key, value) in event_data {
    //             if key == VERSION {
    //                 has_version = true;
    //                 expect_bytes_value(&key, value)?;
    //             } else if key == HOST {
    //                 has_host = true;
    //                 expect_bytes_value(&key, value)?;
    //             } else if key == log_schema().message_key() {
    //                 has_message = true;
    //                 expect_bytes_value(&key, value)?;
    //             } else if key == FULL_MESSAGE || key == FACILITY || key == FILE {
    //                 expect_bytes_value(&key, value)?;
    //             } else if key == TIMESTAMP {
    //                 if !value.is_timestamp() || value.is_integer() {
    //                     return Err(EventGelfConformity::Unconformable(
    //                         format!(
    //                             "LogEvent field {} should be a timestamp type or integer",
    //                             log_schema().timestamp_key()
    //                         )
    //                         .into(),
    //                     ));
    //                 }
    //             } else if key == LEVEL || key == FILE {
    //                 expect_number_value(&key, value)?;
    //             } else {
    //                 if key.len() > 0 && key.chars().nth(0).unwrap() != '_' {
    //                     return Err(EventGelfConformity::Conformable(
    //                         format!("LogEvent field {} is not underscore prefixed", key).into(),
    //                     ));
    //                 }
    //                 if !self.regex.is_match(key) {
    //                     return Err(EventGelfConformity::Conformable(
    //                         format!("LogEvent field {} contains an invalid character", key).into(),
    //                     ));
    //                 }
    //             }
    //         }
    //     }

    //     if !has_version {
    //         Err(EventGelfConformity::Unconformable(
    //             format!("LogEvent does not contain field {}", VERSION).into(),
    //         ))
    //     } else if !has_host {
    //         Err(EventGelfConformity::Unconformable(
    //             format!("LogEvent does not contain field {}", HOST).into(),
    //         ))
    //     } else if !has_message {
    //         Err(EventGelfConformity::Unconformable(
    //             format!(
    //                 "LogEvent does not contain field {}",
    //                 log_schema().message_key()
    //             )
    //             .into(),
    //         ))
    //     } else {
    //         Ok(())
    //     }
    // }

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
                if let Some(clog) = clog {
                    // key is present per caller logic
                    let c_val = clog.get_mut(key).unwrap();
                    *c_val = Value::Bytes(value.coerce_to_bytes());
                    *conformed = true;
                } else {
                    return Err(EventGelfConformity::Conformable(
                        format!("LogEvent field {} should be a UTF-8 string", key).into(),
                    ));
                }
            } else {
                return Err(EventGelfConformity::Unconformable(
                    format!("LogEvent field {} should be a UTF-8 string", key).into(),
                ));
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
        if !value.is_integer() {
            // if the value is a string and that string can be parse into an integer
            if value.is_bytes() {
                let v = value.as_bytes().unwrap();
                match std::str::from_utf8(&v) {
                    Ok(int_str) => {
                        match int_str.parse::<i64>() {
                            Ok(integer) => {
                                if let Some(clog) = clog {
                                    // key is present per caller logic
                                    let c_val = clog.get_mut(key).unwrap();
                                    *c_val = Value::Integer(integer);
                                    *conformed = true;
                                } else {
                                    return Err(EventGelfConformity::Conformable(
                                        format!("LogEvent field {} should be an integer", key)
                                            .into(),
                                    ));
                                }
                            }
                            Err(_) => {
                                return Err(EventGelfConformity::Unconformable(
                                    format!("LogEvent field {} should be an integer", key).into(),
                                ))
                            }
                        }
                    }
                    Err(_) => {
                        return Err(EventGelfConformity::Unconformable(
                            format!("LogEvent field {} should be an integer", key).into(),
                        ))
                    }
                }
            }
            // round off floats
            else if value.is_float() {
                if let Some(clog) = clog {
                    // key is present per caller logic
                    let c_val = clog.get_mut(key).unwrap();
                    *c_val = Value::Integer(value.as_float().unwrap().round() as i64);
                    *conformed = true;
                } else {
                    return Err(EventGelfConformity::Conformable(
                        format!("LogEvent field {} should be an integer", key).into(),
                    ));
                }
            }
            // false -> 0 , true -> 1
            else if value.is_boolean() {
                if let Some(clog) = clog {
                    // key is present per caller logic
                    let c_val = clog.get_mut(key).unwrap();
                    *c_val = Value::Integer(value.as_boolean().unwrap() as i64);
                    *conformed = true;
                } else {
                    return Err(EventGelfConformity::Conformable(
                        format!("LogEvent field {} should be an integer", key).into(),
                    ));
                }
            } else {
                return Err(EventGelfConformity::Unconformable(
                    format!("LogEvent field {} should be an integer", key).into(),
                ));
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
        // VERSION, HOST and <MESSAGE> are all required fields
        if !log.contains(VERSION) {
            return Err(EventGelfConformity::Unconformable(
                format!("LogEvent does not contain field {}", VERSION).into(),
            ));
        }
        if !log.contains(HOST) {
            return Err(EventGelfConformity::Unconformable(
                format!("LogEvent does not contain field {}", HOST).into(),
            ));
        }

        let message_key = log_schema().message_key();
        if !log.contains(message_key) {
            return Err(EventGelfConformity::Unconformable(
                format!(
                    "LogEvent does not contain field {}",
                    log_schema().message_key()
                )
                .into(),
            ));
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
                    self.expect_bytes_value(&key, value, &mut conformed_log, &mut conformed)?;
                }
                // validate timestamp value
                else if key == TIMESTAMP {
                    if !value.is_timestamp() || value.is_integer() {
                        return Err(EventGelfConformity::Unconformable(
                            format!(
                                "LogEvent field {} should be a timestamp type or integer",
                                log_schema().timestamp_key()
                            )
                            .into(),
                        ));
                    }
                }
                // validate integer values
                else if key == LEVEL || key == FILE {
                    self.expect_integer_value(&key, value, &mut conformed_log, &mut conformed)?;
                } else {
                    // additional fields must be prefixed with underscores
                    // NOTE: electing to conform on this rule even if the sanitize flag is not set
                    // because otherwise vector-added fields (such as "source_type: will throw errors
                    if key.len() > 0 && key.chars().nth(0).unwrap() != '_' {
                        if conformed_log.is_none() {
                            let mut clog = log.clone();
                            clog.rename_key(key.as_str(), &*format!("_{}", &key));
                            conformed_log = Some(clog);
                            conformed = true;
                        }
                    }

                    // additional fields must be only word chars, dashes and periods.
                    if !self.valid_regex.is_match(key) {
                        // replace offending characters with dashes
                        if let Some(clog) = &mut conformed_log {
                            let new_key = self.invalid_regex.replace_all(&key, "-");
                            clog.rename_key(key.as_str(), &*new_key);
                            conformed = true;
                        } else {
                            return Err(EventGelfConformity::Conformable(
                                format!("LogEvent field {} contains an invalid character", key)
                                    .into(),
                            ));
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

        match self.is_event_valid_gelf(&log) {
            Ok(conformed) => {
                if let Some(conformed) = conformed {
                    serde_json::to_writer(writer, &conformed)?;
                } else {
                    serde_json::to_writer(writer, &log)?;
                }
                Ok(())
            }
            Err(conformity) => match conformity {
                EventGelfConformity::Conformable(s) => {
                    Err(format!("Event does not conform to GELF specification but is sanitizable, try setting the sanitize configuration option to 'true' for the encoder: {}", s).into())
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

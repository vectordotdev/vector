use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, Utc};
use lookup::path;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::{smallvec, SmallVec};
use std::collections::HashMap;
use value::Kind;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    schema,
};

use super::Deserializer;

/// GELF Message fields. Definitions from https://docs.graylog.org/docs/gelf
pub mod gelf_fields {

    /// <not a field> The latest version of the GELF specificaiton.
    pub const GELF_VERSION: &str = "1.1";

    /// (required) GELF spec version – “1.1”.
    pub const VERSION: &str = "version";

    /// (required) The name of the host, source or application that sent this message.
    pub const HOST: &str = "host";

    /// (required) A short descriptive message.
    pub const SHORT_MESSAGE: &str = "short_message";

    /// (optional) A long message that can i.e. contain a backtrace
    pub const FULL_MESSAGE: &str = "full_message";

    /// (optional) Seconds since UNIX epoch with optional decimal places for milliseconds.
    ///  SHOULD be set by client library. Will be set to the current timestamp (now) by the server if absent.
    pub const TIMESTAMP: &str = "timestamp";

    /// (optional) The level equal to the standard syslog levels. default is 1 (ALERT).
    pub const LEVEL: &str = "level";

    /// (optional) (deprecated) Send as additional field instead.
    pub const FACILITY: &str = "facility";

    /// (optional) (deprecated) The line in a file that caused the error (decimal). Send as additional field instead.
    pub const LINE: &str = "line";

    /// (optional) (deprecated) The file (with path if you want) that caused the error. Send as additional field instead.
    pub const FILE: &str = "file";
}
pub use gelf_fields::*;

/// Config used to build a `GelfDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GelfDeserializerConfig;

impl GelfDeserializerConfig {
    /// Build the `GelfDeserializer` from this configuration.
    pub fn build(&self) -> GelfDeserializer {
        GelfDeserializer::default()
    }

    /// Return the type of event built by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self) -> schema::Definition {
        schema::Definition::empty()
            .with_field(VERSION, Kind::bytes(), None)
            .with_field(HOST, Kind::bytes(), None)
            .with_field(SHORT_MESSAGE, Kind::bytes(), None)
            .optional_field(FULL_MESSAGE, Kind::bytes(), None)
            .optional_field(TIMESTAMP, Kind::timestamp(), None)
            .optional_field(LEVEL, Kind::integer(), None)
            .optional_field(FACILITY, Kind::bytes(), None)
            .optional_field(LINE, Kind::integer(), None)
            .optional_field(FILE, Kind::bytes(), None)
            // Every field with an underscore (_) prefix will be treated as an additional field.
            // Allowed characters in field names are any word character (letter, number, underscore), dashes and dots.
            // Libraries SHOULD not allow to send id as additional field ( _id). Graylog server nodes omit this field automatically.
            .unknown_fields(Kind::bytes())
        //
    }
}

/// Deserializer that builds an `Event` from a byte frame containing a GELF log
/// message.
#[derive(Debug, Clone)]
pub struct GelfDeserializer {
    regex: Regex,
}

impl Default for GelfDeserializer {
    fn default() -> Self {
        Self::new()
    }
}

impl GelfDeserializer {
    /// Create a new GelfDeserializer
    pub fn new() -> GelfDeserializer {
        GelfDeserializer {
            regex: Regex::new(r"^[\w\.\-]*$").unwrap(),
        }
    }

    /// Returns a UTC DateTime from a numeric Value.
    fn parse_timestamp(&self, val: &Value) -> DateTime<Utc> {
        let mut secs = 0;
        let mut nsecs = 0;
        if val.is_f64() {
            let val = val.as_f64().unwrap();
            secs = f64::trunc(val) as i64;
            nsecs = f64::fract(val) as u32;
        } else if val.is_i64() {
            secs = val.as_i64().unwrap();
        } else if val.is_u64() {
            secs = val.as_u64().unwrap() as i64;
        }
        let naive = NaiveDateTime::from_timestamp(secs, nsecs);
        DateTime::<Utc>::from_utc(naive, Utc)
    }

    /// Attemps to parse an integer from a Value.
    fn parse_number(&self, val: &Value) -> Result<i64, vector_core::Error> {
        if val.is_number() {
            let mut number = 0;
            if val.is_f64() {
                number = f64::round(val.as_f64().unwrap()) as i64;
            } else if val.is_u64() {
                number = val.as_u64().unwrap() as i64;
            } else if val.is_i64() {
                number = val.as_i64().unwrap();
            }
            Ok(number)
        } else if val.is_string() {
            let val = val.as_str().unwrap();
            val.parse::<i64>()
                .or_else(|_| val.parse::<f64>().map(|number| number.round() as i64))
                .map_err(|_| {
                    Err(format!(
                        "Event field {} does not match GELF spec version {}: must be a number",
                        VERSION, GELF_VERSION
                    ))
                })
                .into();
        } else {
            Err(format!(
                "Event field {} does not match GELF spec version {}: must be a number",
                VERSION, GELF_VERSION
            )
            .into())
        }
    }

    /// Builds a LogEvent from the parsed GelfMessage.
    /// The logic does not follow strictly the documented GELF standard, it more closely
    /// follows the behavior of graylog itself, which is more relaxed.
    fn message_to_event(&self, parsed: &GelfMessage) -> vector_core::Result<Event> {
        match (&parsed.short_message, &parsed.message) {
            (Some(message), _) | (_, Some(message)) => message,
            _ => {
                return Err("Event must contain the field 'short_message'".into());
            }
        }
        let mut event = Event::from(message.to_string());
        let log = event.as_mut_log();

        let timestamp_key = log_schema().timestamp_key();

        // GELF spec defines the version as 1.1 which has not changed since 2013
        // but graylog server does not reject any event which does not specify a version

        if let Some(host) = &parsed.host {
            log.insert(HOST, host.to_string());
        }
        // TODO graylog sets field 'host' to IP address if not specified. I'm not seeing a clear way to get the IP...

        // FACILITY, FILE, FULL_MESSAGE can be any type and are optional

        if let Some(add) = &parsed.additional_fields {
            for (key, val) in add.iter() {
                // Additional field names must be characters dashes or dots.
                // Drop fields names that contain offending characters.
                if !self.regex.is_match(key) {
                    continue;
                }

                // if the timestamp is not numeric, we set it to current UTC later
                if key == TIMESTAMP && val.is_number() {
                    log.insert(timestamp_key, self.parse_timestamp(val));
                }
                // level and line are both numbers
                else if key == LEVEL || key == LINE {
                    log.insert(path!(key), self.parse_number(val)?);
                } else {
                    let vector_val: value::Value = val.into();

                    // trim the leading underscore prefix if present
                    if let Some(stripped) = key.strip_prefix('_') {
                        log.insert(path!(stripped), vector_val);
                    } else {
                        log.insert(path!(key.as_str()), vector_val);
                    }
                }
            }
        }

        // add a timestamp if not present
        if !log.contains(timestamp_key) {
            log.insert(timestamp_key, Utc::now());
        }

        Ok(event)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct GelfMessage {
    host: Option<String>,
    short_message: Option<String>,
    message: Option<String>,
    #[serde(flatten)]
    additional_fields: Option<HashMap<String, serde_json::Value>>,
}

impl Deserializer for GelfDeserializer {
    fn parse(&self, bytes: Bytes) -> vector_core::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes)?;
        let line = line.trim();

        let parsed: GelfMessage = serde_json::from_str(line)?;
        let event = self.message_to_event(&parsed)?;

        Ok(smallvec![event])
    }
}

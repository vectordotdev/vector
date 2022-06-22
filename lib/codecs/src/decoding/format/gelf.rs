use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, Utc};
use lookup::path;
use regex::Regex;
use serde::{Deserialize, Serialize};
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

    // Builds a LogEvent from the parsed GelfMessage, adhering to GELF spec
    fn insert_fields_from_gelf(&self, parsed: &GelfMessage) -> vector_core::Result<Event> {
        let mut event = Event::from(parsed.short_message.to_string());
        let log = event.as_mut_log();

        // GELF spec defines the version as 1.1 which has not changed since 2013
        if parsed.version != GELF_VERSION {
            return Err(format!(
                "{} does not match GELF spec version ({})",
                VERSION, GELF_VERSION
            )
            .into());
        }

        log.insert(VERSION, parsed.version.to_string());
        log.insert(HOST, parsed.host.to_string());

        if let Some(full_message) = &parsed.full_message {
            log.insert(FULL_MESSAGE, full_message.to_string());
        }

        if let Some(timestamp) = parsed.timestamp {
            let naive = NaiveDateTime::from_timestamp(
                f64::trunc(timestamp) as i64,
                f64::fract(timestamp) as u32,
            );
            log.insert(
                log_schema().timestamp_key(),
                DateTime::<Utc>::from_utc(naive, Utc),
            );
        } else {
            log.insert(log_schema().timestamp_key(), Utc::now());
        }

        if let Some(level) = parsed.level {
            log.insert(LEVEL, level);
        }
        if let Some(facility) = &parsed.facility {
            log.insert(FACILITY, facility.to_string());
        }
        if let Some(line) = &parsed.line {
            log.insert(LINE, *line);
        }
        if let Some(file) = &parsed.file {
            log.insert(FILE, file.to_string());
        }

        if let Some(add) = &parsed.additional_fields {
            for (key, val) in add.iter() {
                // per GELF spec, filter out _id
                if key == "_id" {
                    continue;
                }
                // per GELF spec, Additional field names must be characters dashes or dots

                // drop fields names that don't match the GELF spec
                if !self.regex.is_match(key) {
                    continue;
                }

                // per GELF spec, values to additional fields can be strings or numbers
                if val.is_string() {
                    log.insert(path!(key.as_str()), val.as_str());
                } else if val.is_u64() {
                    log.insert(path!(key.as_str()), val.as_u64());
                } else if val.is_i64() {
                    log.insert(path!(key.as_str()), val.as_i64());
                } else if val.is_f64() {
                    match ordered_float::NotNan::new(val.as_f64().unwrap()) {
                        Ok(float) => {
                            log.insert(path!(key.as_str()), float);
                        }
                        Err(e) => return Err(e.to_string().into()),
                    }
                }
            }
        }

        Ok(event)
    }
}

#[derive(Serialize, Deserialize)]
struct GelfMessage {
    version: String,
    host: String,
    short_message: String,
    full_message: Option<String>,
    timestamp: Option<f64>,
    level: Option<u8>,
    facility: Option<String>,
    line: Option<usize>,
    file: Option<String>,
    #[serde(flatten)]
    additional_fields: Option<HashMap<String, serde_json::Value>>,
}

impl Deserializer for GelfDeserializer {
    fn parse(&self, bytes: Bytes) -> vector_core::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes)?;
        let line = line.trim();

        let parsed: GelfMessage = serde_json::from_str(line)?;
        let event = self.insert_fields_from_gelf(&parsed)?;

        Ok(smallvec![event])
    }
}

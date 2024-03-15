use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::{Event, LogEvent}, schema};
use chrono::{DateTime, SecondsFormat, Local};
use vrl::value::{ObjectMap, Value};
use vector_config::configurable_component;

use std::collections::HashMap;
use std::str::FromStr;
use strum::{FromRepr, EnumString};

/// Config used to build a `SyslogSerializer`.
#[configurable_component]
// Serde default makes all config keys optional.
// Each field assigns either a fixed value, or field name (lookup field key to retrieve dynamic value per `LogEvent`).
#[serde(default)]
#[derive(Clone, Debug, Default)]
pub struct SyslogSerializerConfig {
    /// RFC
    rfc: SyslogRFC,
    /// Facility
    facility: String,
    /// Severity
    severity: String,

    /// App Name
    app_name: Option<String>,
    /// Proc ID
    proc_id: Option<String>,
    /// Msg ID
    msg_id: Option<String>,

    /// Payload key
    payload_key: String,
    /// Add log source
    add_log_source: bool,

    // NOTE: The `tag` field was removed, it is better represented by the equivalents in RFC 5424.
    // Q: The majority of the fields above pragmatically only make sense as config for keys to query?
    // Q: What was `trim_prefix` for? It is not used in file, nor in Vector source tree.
    // Q: `add_log_source` doesn't belong here? Better handled by the `remap` transform with structured data?
}

impl SyslogSerializerConfig {
    /// Build the `SyslogSerializer` from this configuration.
    pub fn build(&self) -> SyslogSerializer {
        SyslogSerializer::new(&self)
    }

    /// The data type of events that are accepted by `SyslogSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the Syslog format.
#[derive(Debug, Clone)]
pub struct SyslogSerializer {
    config: SyslogSerializerConfig
}

impl SyslogSerializer {
    /// Creates a new `SyslogSerializer`.
    pub fn new(conf: &SyslogSerializerConfig) -> Self {
        Self { config: conf.clone() }
    }
}

impl Encoder<Event> for SyslogSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        if let Event::Log(log_event) = event {
            let syslog_message = ConfigDecanter::new(log_event).decant_config(&self.config);

            let vec = syslog_message
                .encode(&self.config.rfc)
                .as_bytes()
                .to_vec();
            buffer.put_slice(&vec);
        }

        Ok(())
    }
}

// Adapts a `LogEvent` into a `SyslogMessage` based on config from `SyslogSerializerConfig`:
// - Splits off the responsibility of encoding logic to `SyslogMessage` (which is not dependent upon Vector types).
// - Majority of methods are only needed to support the `decant_config()` operation.
struct ConfigDecanter {
    log: LogEvent,
}

impl ConfigDecanter {
    fn new(log: LogEvent) -> Self {
        Self {
            log,
        }
    }

    fn decant_config(&self, config: &SyslogSerializerConfig) -> SyslogMessage {
        let x = |v| self.replace_if_proxied(v).unwrap_or_default();
        let facility = x(&config.facility);
        let severity = x(&config.severity);

        let y = |v| self.replace_if_proxied_opt(v);
        let app_name = y(&config.app_name).unwrap_or("vector".to_owned());
        let proc_id = y(&config.proc_id);
        let msg_id = y(&config.msg_id);

        SyslogMessage {
            pri: Pri::from_str_variants(&facility, &severity),
            timestamp: self.get_timestamp(),
            hostname: self.value_by_key("hostname"),
            tag: Tag {
                app_name,
                proc_id,
                msg_id,
            },
            structured_data: self.get_structured_data(),
            message: self.get_message(&config),
        }
    }

    fn replace_if_proxied_opt(&self, value: &Option<String>) -> Option<String> {
        value.as_ref().and_then(|v| self.replace_if_proxied(v))
    }

    // When the value has the expected prefix, perform a lookup for a field key without that prefix part.
    // A failed lookup returns `None`, while a value without the prefix uses the config value as-is.
    //
    // Q: Why `$.message.` as the prefix? (Appears to be JSONPath syntax?)
    // NOTE: Originally named in PR as: `get_field_or_config()`
    fn replace_if_proxied(&self, value: &str) -> Option<String> {
        value
            .strip_prefix("$.message.")
            .map_or(
                Some(value.to_owned()),
                |field_key| self.value_by_key(field_key),
            )
    }

    // NOTE: Originally named in PR as: `get_field()`
    // Now returns a `None` directly instead of converting to either `"-"` or `""`
    fn value_by_key(&self, field_key: &str) -> Option<String> {
        self.log.get(field_key).and_then(|field_value| {
            let bytes = field_value.coerce_to_bytes();
            String::from_utf8(bytes.to_vec()).ok()
        })
    }

    fn get_structured_data(&self) -> Option<StructuredData> {
        self.log.get("structured_data")
          .and_then(|v| v.clone().into_object())
          .map(StructuredData::from)
    }

    fn get_timestamp(&self) -> DateTime::<Local> {
        // Q: Was this Timestamp key hard-coded to the needs of the original PR author?
        //
        // Key `@timestamp` depends on input:
        // https://vector.dev/guides/level-up/managing-schemas/#example-custom-timestamp-field
        // https://vector.dev/docs/about/under-the-hood/architecture/data-model/log/#timestamps
        // NOTE: Log schema key renaming is unavailable when Log namespacing is enabled:
        // https://vector.dev/docs/reference/configuration/global-options/#log_schema
        //
        // NOTE: Log namespacing has metadata `%vector.ingest_timestamp` from a source (file/demo_logs) instead of `timestamp`.
        // As a `payload_key` it will not respect config `encoding.timestamp_format`, but does when
        // using the parent object (`%vector`). Inputs without namespacing respect that config setting.
        if let Some(Value::Timestamp(timestamp)) = self.log.get("@timestamp") {
            // Q: Utc type returned is changed to Local?
            // - Could otherwise return `*timestamp` as-is? Why is Local conversion necessary?
            DateTime::<Local>::from(*timestamp)
        } else {
            // NOTE: Local time is encouraged by RFC 5424 when creating a fallback timestamp for RFC 3164
            Local::now()
        }
    }

    fn get_message(&self, config: &SyslogSerializerConfig) -> String {
        let mut message = String::new();

        if config.add_log_source {
            message.push_str(self.add_log_source().as_str());
        }

        // `payload_key` configures where to source the value for the syslog `message`:
        // - Field key (Valid)   => Get value by lookup (value_by_key)
        // - Field key (Invalid) => Empty string (unwrap_or_default)
        // - Not configured      => JSON encoded `LogEvent` (fallback?)
        //
        // Q: Was the JSON fallback intended by the original PR author only for debugging?
        //    Roughly equivalent to using `payload_key: .` (in YAML config)?
        let payload = if config.payload_key.is_empty() {
            serde_json::to_string(&self.log).ok()
        } else {
            self.value_by_key(&config.payload_key)
        };

        message.push_str(&payload.unwrap_or_default());
        message
    }

    // NOTE: This is a third-party addition from the original PR author (it is not relevant to the syslog spec):
    // TODO: Remove, as this type of additional data is better supported via VRL remap + `StructuredData`?
    fn add_log_source(&self) -> String {
        let get_value = |s| self.value_by_key(s).unwrap_or_default();

        [
            "namespace_name=", get_value("kubernetes.namespace_name").as_str(),
            ", container_name=", get_value("kubernetes.container_name").as_str(),
            ", pod_name=", get_value("kubernetes.pod_name").as_str(),
            ", message="
        ].concat()
    }
}

//
// SyslogMessage support
//

const NIL_VALUE: &'static str = "-";
const SYSLOG_V1: &'static str = "1";

/// Syslog RFC
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyslogRFC {
    /// RFC 3164
    Rfc3164,

    #[default]
    /// RFC 5424
    Rfc5424
}

// ABNF definition:
// https://datatracker.ietf.org/doc/html/rfc5424#section-6
// https://datatracker.ietf.org/doc/html/rfc5424#section-6.2
#[derive(Default, Debug)]
struct SyslogMessage {
    pri: Pri,
    timestamp: DateTime::<Local>,
    hostname: Option<String>,
    tag: Tag,
    structured_data: Option<StructuredData>,
    message: String,
}

impl SyslogMessage {
    fn encode(&self, rfc: &SyslogRFC) -> String {
        // Q: NIL_VALUE is unlikely? Technically invalid for RFC 3164:
        // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.4
        // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.2
        let hostname = self.hostname.as_deref().unwrap_or(NIL_VALUE);
        let structured_data = self.structured_data.as_ref().map(|sd| sd.encode());

        let fields_encoded = match rfc {
            SyslogRFC::Rfc3164 => {
                // TIMESTAMP field format:
                // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.2
                let timestamp = self.timestamp.format("%b %e %H:%M:%S").to_string();
                // MSG part begins with TAG field + optional context:
                // https://datatracker.ietf.org/doc/html/rfc3164#section-4.1.3
                let mut msg_start = self.tag.encode_rfc_3164();
                // When RFC 5424 "Structured Data" is available, it can be compatible with RFC 3164
                // by including it in the RFC 3164 `CONTENT` field (part of MSG):
                // https://datatracker.ietf.org/doc/html/rfc5424#appendix-A.1
                if let Some(sd) = structured_data.as_deref() {
                    msg_start = msg_start + " " + sd
                }

                [
                    timestamp.as_str(),
                    hostname,
                    &msg_start,
                ].join(" ")
            },
            SyslogRFC::Rfc5424 => {
                // HEADER part fields:
                // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2
                let version = SYSLOG_V1;
                let timestamp = self.timestamp.to_rfc3339_opts(SecondsFormat::Millis, true);
                let tag = self.tag.encode_rfc_5424();
                let sd = structured_data.as_deref().unwrap_or(NIL_VALUE);

                [
                    version,
                    timestamp.as_str(),
                    hostname,
                    &tag,
                    sd
                ].join(" ")
            }
        };

        [
            &self.pri.encode(),
            &fields_encoded,
            " ",
            &self.message,
        ].concat()

        // Q: RFC 5424 MSG part should technically ensure UTF-8 message begins with BOM?
        // https://datatracker.ietf.org/doc/html/rfc5424#section-6.4
    }
}

#[derive(Default, Debug)]
struct Tag {
    app_name: String,
    proc_id: Option<String>,
    msg_id: Option<String>
}

// NOTE: `.as_deref()` usage below avoids requiring `self.clone()`
impl Tag {
    // Roughly equivalent - RFC 5424 fields can compose the start of
    // an RFC 3164 MSG part (TAG + CONTENT fields):
    // https://datatracker.ietf.org/doc/html/rfc5424#appendix-A.1
    fn encode_rfc_3164(&self) -> String {
        let Self { app_name, proc_id, msg_id } = self;

        match proc_id.as_deref().or(msg_id.as_deref()) {
            Some(context) => [&app_name, "[", &context, "]:"].concat(),
            None => [&app_name, ":"].concat()
        }
    }

    // TAG was split into separate fields: APP-NAME, PROCID, MSGID
    // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.5
    fn encode_rfc_5424(&self) -> String {
        let Self { app_name, proc_id, msg_id } = self;

        [
            &app_name,
            proc_id.as_deref().unwrap_or(NIL_VALUE),
            msg_id.as_deref().unwrap_or(NIL_VALUE),
        ].join(" ")
    }
}

// Structured Data:
// https://datatracker.ietf.org/doc/html/rfc5424#section-6.3
// An SD-ELEMENT consists of a name (SD-ID) + parameter key-value pairs (SD-PARAM)
type StructuredDataMap = HashMap<String, HashMap<String, String>>;
#[derive(Debug, Default)]
struct StructuredData {
    elements: StructuredDataMap
}

// Used by `SyslogMessage::encode()`
/*
  Adapted `format_structured_data_rfc5424` method from:
  https://github.com/vectordotdev/vector/blob/fafe8c50a4721fa3ddbea34e0641d3c145f14388/src/sources/syslog.rs#L1548-L1563

  No notable change in logic, uses `NIL_VALUE` constant, and adapts method to struct instead of free-standing.
*/
impl StructuredData {
    fn encode(&self) -> String {
        if self.elements.is_empty() {
            NIL_VALUE.to_string()
        } else {
            let mut s = String::new();

            for (sd_id, sd_params) in &self.elements {
                s = s + "[" + sd_id;
                for (key, value) in sd_params {
                    s = s + " " + key + "=\"" + value + "\"";
                }
                s += "]";
            }

            s
        }
    }
}

// Used by `ConfigDecanter::decant_config()`
/*
  Adapted `structured_data_from_fields()` method from:
  https://github.com/vectordotdev/vector/blob/fafe8c50a4721fa3ddbea34e0641d3c145f14388/src/sources/syslog.rs#L1439-L1454

  Refactored to `impl From` that uses `flat_map()` instead to collect K/V tuples into a `HashMap`.
*/
impl From<ObjectMap> for StructuredData {
    fn from(fields: ObjectMap) -> Self {
        let elements = fields.into_iter().flat_map(|(sd_id, value)| {
            let sd_params = value
                .into_object()?
                .into_iter()
                .map(|(k, v)| (k.into(), value_to_string(v)))
                .collect();

            Some((sd_id.into(), sd_params))
        }).collect::<StructuredDataMap>();

        Self { elements }
    }
}

// Only used as helper to support `StructuredData::from()`
/*
  Adapted `value_to_string()` method from:
  https://github.com/vectordotdev/vector/blob/fafe8c50a4721fa3ddbea34e0641d3c145f14388/src/sources/syslog.rs#L1569-L1579
  https://github.com/vectordotdev/vrl/blob/main/src/value/value/convert.rs
  https://github.com/vectordotdev/vrl/blob/main/src/value/value/display.rs

  Simplified via `match` expression which seems better suited for this logic.
*/
fn value_to_string(v: Value) -> String {
    match v {
        Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Value::Timestamp(timestamp) => timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        _ => v.to_string()
    }
}

//
// Facility + Severity support
//

#[derive(Default, Debug)]
struct Pri {
    facility: Facility,
    severity: Severity,
}

impl Pri {
    fn from_str_variants(facility_variant: &str, severity_variant: &str) -> Self {
        // The original PR had `deserialize_*()` methods parsed a value to a `u8` or stored a field key as a `String`
        // Later the equivalent `get_num_*()` method would retrieve the `u8` value or lookup the field key for the actual value,
        // otherwise it'd fallback to the default Facility/Severity value.
        // This approach instead parses a string of the name or ordinal representation,
        // any reference via field key lookup should have already happened by this point.
        let facility = Facility::into_variant(&facility_variant).unwrap_or(Facility::User);
        let severity = Severity::into_variant(&severity_variant).unwrap_or(Severity::Informational);

        Self {
            facility,
            severity,
        }
    }

    // The last paragraph describes how to compose the enums into `PRIVAL`:
    // https://datatracker.ietf.org/doc/html/rfc5424#section-6.2.1
    fn encode(&self) -> String {
        let prival = (self.facility as u8 * 8) + self.severity as u8;
        ["<", &prival.to_string(), ">"].concat()
    }
}

// Facility + Severity mapping from Name => Ordinal number:

/// Syslog facility
#[derive(Default, Debug, EnumString, FromRepr, Copy, Clone)]
#[strum(serialize_all = "kebab-case")]
enum Facility {
    Kern = 0,
    #[default]
    User = 1,
    Mail = 2,
    Daemon = 3,
    Auth = 4,
    Syslog = 5,
    LPR = 6,
    News = 7,
    UUCP = 8,
    Cron = 9,
    AuthPriv = 10,
    FTP = 11,
    NTP = 12,
    Security = 13,
    Console = 14,
    SolarisCron = 15,
    Local0 = 16,
    Local1 = 17,
    Local2 = 18,
    Local3 = 19,
    Local4 = 20,
    Local5 = 21,
    Local6 = 22,
    Local7 = 23,
}

/// Syslog severity
#[derive(Default, Debug, EnumString, FromRepr, Copy, Clone)]
#[strum(serialize_all = "kebab-case")]
enum Severity {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    #[default]
    Informational = 6,
    Debug = 7,
}

// Additionally support variants from string-based integers:
// Parse a string name, with fallback for parsing a string ordinal number.
impl Facility {
    fn into_variant(variant_name: &str) -> Option<Self> {
        let s = variant_name.to_ascii_lowercase();

        s.parse::<usize>().map_or_else(
            |_| Self::from_str(&s).ok(),
            |num| Self::from_repr(num),
        )
    }
}

// NOTE: The `strum` crate does not provide traits,
// requiring copy/paste of the prior impl instead.
impl Severity {
    fn into_variant(variant_name: &str) -> Option<Self> {
        let s = variant_name.to_ascii_lowercase();

        s.parse::<usize>().map_or_else(
            |_| Self::from_str(&s).ok(),
            |num| Self::from_repr(num),
        )
    }
}

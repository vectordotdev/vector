use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::{Event, LogEvent}, schema};
use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use vrl::value::{ObjectMap, Value};
use vector_config::configurable_component;

use std::collections::HashMap;
use std::str::FromStr;
use strum::{FromRepr, EnumString};
use akin::akin;

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

    // Q: The majority of the fields above pragmatically only make sense as config for keys to query?
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

    fn get_timestamp(&self) -> DateTime::<Utc> {
        // Q: Should the timestamp source be configurable? (eg: Select a field from the `remap` transform)
        //
        // Concerns:
        // - A source with `log_namespace: true` seems to cause `get_timstamp()` to return `None`?
        // Does not seem to retrieve `%vector.ingest_timestamp`?
        // - A sink type `console` with `timestamp_format: unix_ms` converts a `Value::Timestamp` prior to the encoder logic
        // to `Value::Integer(i64)` instead, which won't match this condition.
        //
        // NOTE:
        // Vector always manages `Value::Timestamp` as `DateTime<Utc>`, any prior TZ information context is always dropped.
        // If restoring the TZ for a log is important, it could be handled via a remap transform?
        //
        // Ref:
        // `log.get_timestamp()`:
        // https://github.com/vectordotdev/vector/blob/ad6a48efc0f79b2c18a5c1394e5d8603fdfd1bab/lib/vector-core/src/event/log_event.rs#L543-L552
        if let Some(Value::Timestamp(timestamp)) = self.log.get_timestamp() {
            *timestamp
        } else {
            // NOTE:
            // When timezone information is missing Vector handles conversion to UTC by assuming the local TZ:
            // https://vector.dev/docs/about/under-the-hood/architecture/data-model/log/#time-zones
            // There is a global option for which TZ to assume (where the default is local TZ):
            // https://vector.dev/docs/reference/configuration/global-options/#timezone
            // https://github.com/vectordotdev/vector/blob/58a4a2ef52e606c0f9b9fa975cf114b661300584/lib/vector-core/src/config/global_options.rs#L233-L236
            // https://github.com/vectordotdev/vrl/blob/c010300710a00191cd406e57cd0f3e001923d598/src/compiler/datetime.rs#L88-L95
            // VRL remap can also override that:
            // https://vector.dev/docs/reference/configuration/transforms/remap/#timezone
            // Vector's `syslog` source type also uses `Utc::now()` internally as a fallback:
            // https://github.com/vectordotdev/vector/blob/58a4a2ef52e606c0f9b9fa975cf114b661300584/src/sources/syslog.rs#L430-L438
            Utc::now()
        }
    }

    fn get_message(&self, config: &SyslogSerializerConfig) -> String {
        // `payload_key` configures where to source the value for the syslog `message`:
        // - Not configured      => Encodes the default log message.
        // - Field key (Valid)   => Get value by lookup (value_by_key)
        // - Field key (Invalid) => Empty string (unwrap_or_default)

        // Ref:
        // `log.get_message()`:
        // https://github.com/vectordotdev/vector/blob/ad6a48efc0f79b2c18a5c1394e5d8603fdfd1bab/lib/vector-core/src/event/log_event.rs#L532-L541
        // `v.to_string_lossy()`:
        // https://github.com/vectordotdev/vrl/blob/f2d71cd26cb8270230f531945d7dee4929235905/src/value/value/serde.rs#L34-L55
        let payload = if config.payload_key.is_empty() {
            self.log.get_message().map(|v| v.to_string_lossy().to_string() )
        } else {
            self.value_by_key(&config.payload_key)
        };

        payload.unwrap_or_default()
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
    timestamp: DateTime::<Utc>,
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
                // https://docs.rs/chrono/latest/chrono/format/strftime/index.html
                //
                // TODO: Should this remain as UTC or adjust to the local TZ of the environment (or Vector config)?
                // RFC 5424 suggests (when adapting for RFC 3164) to present a timestamp with the local TZ of the log source:
                // https://www.rfc-editor.org/rfc/rfc5424#appendix-A.1
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
                // TIME-FRAC max length is 6 digits (microseconds):
                // https://datatracker.ietf.org/doc/html/rfc5424#section-6
                // TODO: Likewise for RFC 5424, as UTC the offset will always render as `Z` if not configurable.
                let timestamp = self.timestamp.round_subsecs(6).to_rfc3339_opts(SecondsFormat::AutoSi, true);
                let tag = self.tag.encode_rfc_5424();
                let sd = structured_data.as_deref().unwrap_or(NIL_VALUE);

                [
                    SYSLOG_V1,
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
// Attempts to parse a string for ordinal mapping first, otherwise try the variant name.
// NOTE: No error handling in place, invalid config will fallback to default during `decant_config()`.
akin! {
    let &enums = [Facility, Severity];

    impl *enums {
        fn into_variant(variant_name: &str) -> Option<Self> {
            let s = variant_name.to_ascii_lowercase();

            s.parse::<usize>().map_or_else(
                |_| Self::from_str(&s).ok(),
                |num| Self::from_repr(num),
            )
        }
    }
}

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
                // Structured Data:
                // https://datatracker.ietf.org/doc/html/rfc5424#section-6.3
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

#[derive(Debug)]
struct StructuredData {}

impl StructuredData {
    fn encode(&self) -> String {
        todo!()
    }
}

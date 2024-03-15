const NIL_VALUE: &'static str = "-";

/// Syslog RFC
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyslogRFC {
    /// RFC 3164
    Rfc3164,

    /// RFC 5424
    Rfc5424
}

impl Default for SyslogRFC {
    fn default() -> Self {
        SyslogRFC::Rfc5424
    }
}

/// Config used to build a `SyslogSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct SyslogSerializerConfig {
    /// RFC
    #[serde(default)]
    rfc: SyslogRFC,

    /// Facility
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_facility")]
    facility: Facility,

    /// Severity
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_severity")]
    severity: Severity,

    /// Tag
    #[serde(default)]
    tag: String,

    /// Trim prefix
    trim_prefix: Option<String>,

    /// Payload key
    #[serde(default)]
    payload_key: String,

    /// Add log source
    #[serde(default)]
    add_log_source: bool,

    /// App Name, RFC 5424 only
    #[serde(default = "default_app_name")]
    app_name: String,

    /// Proc ID, RFC 5424 only
    #[serde(default = "default_nil_value")]
    proc_id: String,

    /// Msg ID, RFC 5424 only
    #[serde(default = "default_nil_value")]
    msg_id: String
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

fn default_app_name() -> String {
    String::from("vector")
}

fn default_nil_value() -> String {
    String::from(NIL_VALUE)
}

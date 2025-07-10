use crate::encoding::BuildError;
use bytes::BytesMut;
use chrono::SecondsFormat;
use lookup::lookup_v2::ConfigTargetPath;
use snafu::Snafu;
use std::num::ParseIntError;
use std::{collections::HashMap, fmt::Write};
use tokio_util::codec::Encoder;
use vector_config_macros::configurable_component;
use vector_core::{
    config::DataType,
    event::{Event, LogEvent, Value},
    schema,
};

const DEFAULT_DEVICE_VENDOR: &str = "Datadog";
const DEFAULT_DEVICE_PRODUCT: &str = "Vector";
// Major version of Vector.
// TODO: find a way to get the actual vector version.
//  The version should be the actual vector version, but it's not possible
//  to get it from the config.
const DEFAULT_DEVICE_VERSION: &str = "0";
const DEFAULT_EVENT_CLASS_ID: &str = "Telemetry Event";
const DEVICE_VENDOR_MAX_LENGTH: usize = 63;
const DEVICE_PRODUCT_MAX_LENGTH: usize = 63;
const DEVICE_VERSION_MAX_LENGTH: usize = 31;
const DEVICE_EVENT_CLASS_ID_MAX_LENGTH: usize = 1023;
const NAME_MAX_LENGTH: usize = 512;
const SEVERITY_MAX: u8 = 10;

/// Represents the device settings in the CEF format.
#[derive(Debug, Clone)]
pub struct DeviceSettings {
    pub vendor: String,
    pub product: String,
    pub version: String,
    pub event_class_id: String,
}

impl DeviceSettings {
    /// Creates a new `DeviceSettings`.
    pub const fn new(
        vendor: String,
        product: String,
        version: String,
        event_class_id: String,
    ) -> Self {
        Self {
            vendor,
            product,
            version,
            event_class_id,
        }
    }
}

/// Errors that can occur during CEF serialization.
#[derive(Debug, Snafu)]
pub enum CefSerializerError {
    #[snafu(display(
        r#"LogEvent field "{}" with the value "{}" exceed {} characters limit: actual {}"#,
        field_name,
        field,
        max_length,
        actual_length
    ))]
    ExceededLength {
        field: String,
        field_name: String,
        max_length: usize,
        actual_length: usize,
    },
    #[snafu(display(
        r#"LogEvent CEF severity must be a number from 0 to {}: actual {}"#,
        max_value,
        actual_value
    ))]
    SeverityMaxValue { max_value: u8, actual_value: u8 },
    #[snafu(display(r#"LogEvent CEF severity must be a number: {}"#, error))]
    SeverityNumberType { error: ParseIntError },
    #[snafu(display(r#"LogEvent extension keys can only contain ascii alphabetical characters: invalid key "{}""#, key))]
    ExtensionNonASCIIKey { key: String },
}

/// Config used to build a `CefSerializer`.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct CefSerializerConfig {
    /// The CEF Serializer Options.
    pub cef: CefSerializerOptions,
}

impl CefSerializerConfig {
    /// Creates a new `CefSerializerConfig`.
    pub const fn new(cef: CefSerializerOptions) -> Self {
        Self { cef }
    }

    /// Build the `CefSerializer` from this configuration.
    pub fn build(&self) -> Result<CefSerializer, BuildError> {
        let device_vendor = validate_length(
            &self.cef.device_vendor,
            "device_vendor",
            DEVICE_VENDOR_MAX_LENGTH,
        )?;
        let device_product = validate_length(
            &self.cef.device_product,
            "device_product",
            DEVICE_PRODUCT_MAX_LENGTH,
        )?;
        let device_version = validate_length(
            &self.cef.device_version,
            "device_version",
            DEVICE_VERSION_MAX_LENGTH,
        )?;
        let device_event_class_id = validate_length(
            &self.cef.device_event_class_id,
            "device_event_class_id",
            DEVICE_EVENT_CLASS_ID_MAX_LENGTH,
        )?;

        let invalid_keys: Vec<String> = self
            .cef
            .extensions
            .keys()
            .filter(|key| !key.chars().all(|c| c.is_ascii_alphabetic()))
            .cloned()
            .collect();

        if !invalid_keys.is_empty() {
            return ExtensionNonASCIIKeySnafu {
                key: invalid_keys.join(", "),
            }
            .fail()
            .map_err(|e| e.to_string().into());
        }

        let device = DeviceSettings::new(
            device_vendor,
            device_product,
            device_version,
            device_event_class_id,
        );

        Ok(CefSerializer::new(
            self.cef.version.clone(),
            device,
            self.cef.severity.clone(),
            self.cef.name.clone(),
            self.cef.extensions.clone(),
        ))
    }

    /// The data type of events that are accepted by `CefSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // CEF, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// CEF version.
#[configurable_component]
#[derive(Debug, Default, Clone)]
pub enum Version {
    #[default]
    /// CEF specification version 0.1.
    V0,
    /// CEF specification version 1.x.
    V1,
}

impl Version {
    fn as_str(&self) -> &'static str {
        match self {
            Version::V0 => "0",
            Version::V1 => "1",
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Config used to build a `CefSerializer`.
#[configurable_component]
#[derive(Debug, Clone)]
pub struct CefSerializerOptions {
    /// CEF Version. Can be either 0 or 1.
    /// Set to "0" by default.
    pub version: Version,

    /// Identifies the vendor of the product.
    /// The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
    /// The value length must be less than or equal to 63.
    pub device_vendor: String,

    /// Identifies the product of a vendor.
    /// The part of a unique device identifier. No two products can use the same combination of device vendor and device product.
    /// The value length must be less than or equal to 63.
    pub device_product: String,

    /// Identifies the version of the problem. The combination of the device product, vendor and this value make up the unique id of the device that sends messages.
    /// The value length must be less than or equal to 31.
    pub device_version: String,

    /// Unique identifier for each event type. Identifies the type of event reported.
    /// The value length must be less than or equal to 1023.
    pub device_event_class_id: String,

    /// This is a path that points to the field of a log event that reflects importance of the event.
    /// Reflects importance of the event.
    ///
    /// It must point to a number from 0 to 10.
    /// 0 = lowest_importance, 10 = highest_importance.
    /// Set to "cef.severity" by default.
    pub severity: ConfigTargetPath,

    /// This is a path that points to the human-readable description of a log event.
    /// The value length must be less than or equal to 512.
    /// Equals "cef.name" by default.
    pub name: ConfigTargetPath,

    /// The collection of key-value pairs. Keys are the keys of the extensions, and values are paths that point to the extension values of a log event.
    /// The event can have any number of key-value pairs in any order.
    #[configurable(metadata(
        docs::additional_props_description = "This is a path that points to the extension value of a log event."
    ))]
    pub extensions: HashMap<String, ConfigTargetPath>,
    // TODO: use Template instead of ConfigTargetPath.
    //   Templates are in the src/ package, and codes are in the lib/codecs.
    //   Moving the Template to the lib/ package in order to prevent the circular dependency.
}

impl Default for CefSerializerOptions {
    fn default() -> Self {
        Self {
            version: Version::default(),
            device_vendor: String::from(DEFAULT_DEVICE_VENDOR),
            device_product: String::from(DEFAULT_DEVICE_PRODUCT),
            device_version: String::from(DEFAULT_DEVICE_VERSION),
            device_event_class_id: String::from(DEFAULT_EVENT_CLASS_ID),
            severity: ConfigTargetPath::try_from("cef.severity".to_string())
                .expect("could not parse path"),
            name: ConfigTargetPath::try_from("cef.name".to_string()).expect("could not parse path"),
            extensions: HashMap::new(),
        }
    }
}

/// Serializer that converts an `Event` to the bytes using the CEF format.
/// CEF:{version}|{device_vendor}|{device_product}|{device_version>|{device_event_class}|{name}|{severity}|{encoded_fields}
#[derive(Debug, Clone)]
pub struct CefSerializer {
    version: Version,
    device: DeviceSettings,
    severity: ConfigTargetPath,
    name: ConfigTargetPath,
    extensions: HashMap<String, ConfigTargetPath>,
}

impl CefSerializer {
    /// Creates a new `CefSerializer`.
    pub const fn new(
        version: Version,
        device: DeviceSettings,
        severity: ConfigTargetPath,
        name: ConfigTargetPath,
        extensions: HashMap<String, ConfigTargetPath>,
    ) -> Self {
        Self {
            version,
            device,
            severity,
            name,
            extensions,
        }
    }
}

impl Encoder<Event> for CefSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();

        let severity: u8 = match get_log_event_value(&log, &self.severity).parse() {
            Err(err) => {
                return SeverityNumberTypeSnafu { error: err }
                    .fail()
                    .map_err(|e| e.to_string().into());
            }
            Ok(severity) => {
                if severity > SEVERITY_MAX {
                    return SeverityMaxValueSnafu {
                        max_value: SEVERITY_MAX,
                        actual_value: severity,
                    }
                    .fail()
                    .map_err(|e| e.to_string().into());
                };
                severity
            }
        };

        let name: String = get_log_event_value(&log, &self.name);
        let name = validate_length(&name, "name", NAME_MAX_LENGTH)?;

        let mut formatted_extensions = Vec::with_capacity(self.extensions.len());
        for (extension, field) in &self.extensions {
            let value = get_log_event_value(&log, field);
            if value.is_empty() {
                continue;
            }
            let value = escape_extension(&value);
            formatted_extensions.push(format!("{}={}", extension, value));
        }

        buffer.write_fmt(format_args!(
            "CEF:{}|{}|{}|{}|{}|{}|{}",
            &self.version,
            &self.device.vendor,
            &self.device.product,
            &self.device.version,
            &self.device.event_class_id,
            name,
            severity,
        ))?;
        if !formatted_extensions.is_empty() {
            formatted_extensions.sort();

            buffer.write_char('|')?;
            buffer.write_str(formatted_extensions.join(" ").as_str())?;
        }

        Ok(())
    }
}

fn get_log_event_value(log: &LogEvent, field: &ConfigTargetPath) -> String {
    match log.get(field) {
        Some(Value::Bytes(bytes)) => String::from_utf8_lossy(bytes).to_string(),
        Some(Value::Integer(int)) => int.to_string(),
        Some(Value::Float(float)) => float.to_string(),
        Some(Value::Boolean(bool)) => bool.to_string(),
        Some(Value::Timestamp(timestamp)) => timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        Some(Value::Null) => String::from(""),
        // Other value types: Array, Regex, Object are not supported by the CEF format.
        Some(_) => String::from(""),
        None => String::from(""),
    }
}

fn escape_header(s: &str) -> String {
    escape_special_chars(s, '|')
}
fn escape_extension(s: &str) -> String {
    escape_special_chars(s, '=')
}

fn escape_special_chars(s: &str, extra_char: char) -> String {
    s.replace('\\', r#"\\"#)
        .replace(extra_char, &format!(r#"\{}"#, extra_char))
}

fn validate_length(field: &str, field_name: &str, max_length: usize) -> Result<String, BuildError> {
    let escaped = escape_header(field);
    if escaped.len() > max_length {
        ExceededLengthSnafu {
            field: escaped.clone(),
            field_name,
            max_length,
            actual_length: escaped.len(),
        }
        .fail()?;
    }
    Ok(escaped)
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use chrono::DateTime;
    use ordered_float::NotNan;
    use vector_common::btreemap;
    use vector_core::event::{Event, LogEvent, Value};

    use super::*;

    #[test]
    fn build_error_on_invalid_extension() {
        let extensions = HashMap::from([(
            String::from("foo.test"),
            ConfigTargetPath::try_from("foo".to_string()).unwrap(),
        )]);
        let opts: CefSerializerOptions = CefSerializerOptions {
            extensions,
            ..CefSerializerOptions::default()
        };
        let config = CefSerializerConfig::new(opts);
        let err = config.build().unwrap_err();
        assert_eq!(
            err.to_string(),
            "LogEvent extension keys can only contain ascii alphabetical characters: invalid key \"foo.test\""
        );
    }

    #[test]
    fn build_error_max_length() {
        let extensions = HashMap::from([(
            String::from("foo-test"),
            ConfigTargetPath::try_from("foo".to_string()).unwrap(),
        )]);
        let opts: CefSerializerOptions = CefSerializerOptions {
            device_vendor: "Repeat".repeat(11), // more than max length
            extensions,
            ..CefSerializerOptions::default()
        };
        let config = CefSerializerConfig::new(opts);
        let err = config.build().unwrap_err();
        assert_eq!(
            err.to_string(),
            "LogEvent field \"device_vendor\" with the value \"RepeatRepeatRepeatRepeatRepeatRepeatRepeatRepeatRepeatRepeatRepeat\" exceed 63 characters limit: actual 66"
        );
    }

    #[test]
    fn try_escape_header() {
        let s1 = String::from(r#"Test | test"#);
        let s2 = String::from(r#"Test \ test"#);
        let s3 = String::from(r#"Test test"#);
        let s4 = String::from(r#"Test \| \| test"#);

        let s1 = escape_header(&s1);
        let s2 = escape_header(&s2);
        let s3: String = escape_header(&s3);
        let s4: String = escape_header(&s4);

        assert_eq!(s1, r#"Test \| test"#);
        assert_eq!(s2, r#"Test \\ test"#);
        assert_eq!(s3, r#"Test test"#);
        assert_eq!(s4, r#"Test \\\| \\\| test"#);
    }

    #[test]
    fn try_escape_extension() {
        let s1 = String::from(r#"Test=test"#);
        let s2 = String::from(r#"Test = test"#);
        let s3 = String::from(r#"Test test"#);
        let s4 = String::from(r#"Test \| \| test"#);

        let s1 = escape_extension(&s1);
        let s2 = escape_extension(&s2);
        let s3: String = escape_extension(&s3);
        let s4: String = escape_extension(&s4);

        assert_eq!(s1, r#"Test\=test"#);
        assert_eq!(s2, r#"Test \= test"#);
        assert_eq!(s3, r#"Test test"#);
        assert_eq!(s4, r#"Test \\| \\| test"#);
    }

    #[test]
    fn serialize_extensions() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "cef" => Value::from(btreemap! {
                "severity" => Value::from(1),
                "name" => Value::from("Event name"),
            }),
            "foo" => Value::from("bar"),
            "int" => Value::from(123),
            "comma" => Value::from("abc,bcd"),
            "float" => Value::Float(NotNan::new(3.1415925).unwrap()),
            "space" => Value::from("sp ace"),
            "time" => Value::Timestamp(DateTime::parse_from_rfc3339("2023-02-27T15:04:49.363+08:00").unwrap().into()),
            "quote" => Value::from("the \"quote\" should be escaped"),
            "bool" => Value::from(true),
            "other" => Value::from("data"),
        }));

        let extensions = HashMap::from([
            (
                String::from("foo"),
                ConfigTargetPath::try_from("foo".to_string()).unwrap(),
            ),
            (
                String::from("int"),
                ConfigTargetPath::try_from("int".to_string()).unwrap(),
            ),
            (
                String::from("comma"),
                ConfigTargetPath::try_from("comma".to_string()).unwrap(),
            ),
            (
                String::from("float"),
                ConfigTargetPath::try_from("float".to_string()).unwrap(),
            ),
            (
                String::from("missing"),
                ConfigTargetPath::try_from("missing".to_string()).unwrap(),
            ),
            (
                String::from("space"),
                ConfigTargetPath::try_from("space".to_string()).unwrap(),
            ),
            (
                String::from("time"),
                ConfigTargetPath::try_from("time".to_string()).unwrap(),
            ),
            (
                String::from("quote"),
                ConfigTargetPath::try_from("quote".to_string()).unwrap(),
            ),
            (
                String::from("bool"),
                ConfigTargetPath::try_from("bool".to_string()).unwrap(),
            ),
        ]);

        let opts: CefSerializerOptions = CefSerializerOptions {
            extensions,
            ..CefSerializerOptions::default()
        };

        let config = CefSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();
        let expected = b"CEF:0|Datadog|Vector|0|Telemetry Event|Event name|1|bool=true comma=abc,bcd float=3.1415925 foo=bar int=123 quote=the \"quote\" should be escaped space=sp ace time=2023-02-27T07:04:49.363Z";

        assert_eq!(bytes.as_ref(), expected);
    }
}

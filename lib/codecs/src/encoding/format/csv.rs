use crate::encoding::BuildError;
use bytes::BytesMut;
use chrono::SecondsFormat;
use lookup::lookup_v2::ConfigTargetPath;
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};

/// Config used to build a `CsvSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct CsvSerializerConfig {
    /// The CSV Serializer Options.
    pub csv: CsvSerializerOptions,
}

impl CsvSerializerConfig {
    /// Creates a new `CsvSerializerConfig`.
    pub const fn new(csv: CsvSerializerOptions) -> Self {
        Self { csv }
    }

    /// Build the `CsvSerializer` from this configuration.
    pub fn build(&self) -> Result<CsvSerializer, BuildError> {
        if self.csv.fields.is_empty() {
            Err("At least one CSV field must be specified".into())
        } else {
            Ok(CsvSerializer::new(self.clone()))
        }
    }

    /// The data type of events that are accepted by `CsvSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // CSV, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// The user configuration to choose the metric tag strategy.
#[crate::configurable_component]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStyle {
    /// This puts quotes around every field. Always.
    Always,

    /// This puts quotes around fields only when necessary.
    /// They are necessary when fields contain a quote, delimiter or record terminator.
    /// Quotes are also necessary when writing an empty record
    /// (which is indistinguishable from a record with one empty field).
    #[default]
    Necessary,

    /// This puts quotes around all fields that are non-numeric.
    /// Namely, when writing a field that does not parse as a valid float or integer,
    /// then quotes will be used even if they aren’t strictly necessary.
    NonNumeric,

    /// This never writes quotes, even if it would produce invalid CSV data.
    Never,
}

/// Config used to build a `CsvSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct CsvSerializerOptions {
    /// The field delimiter to use when writing CSV.
    #[serde(
        default = "default_delimiter",
        with = "vector_core::serde::ascii_char",
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub delimiter: u8,

    /// Enable double quote escapes.
    ///
    /// This is enabled by default, but it may be disabled. When disabled, quotes in
    /// field data are escaped instead of doubled.
    #[serde(
        default = "default_double_quote",
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub double_quote: bool,

    /// The escape character to use when writing CSV.
    ///
    /// In some variants of CSV, quotes are escaped using a special escape character
    /// like \ (instead of escaping quotes by doubling them).
    ///
    /// To use this `double_quotes` needs to be disabled as well otherwise it is ignored
    #[serde(
        default = "default_escape",
        with = "vector_core::serde::ascii_char",
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub escape: u8,

    /// The quote character to use when writing CSV.
    #[serde(
        default = "default_escape",
        with = "vector_core::serde::ascii_char",
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    quote: u8,

    /// The quoting style to use when writing CSV data.
    #[serde(
        default,
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub quote_style: QuoteStyle,

    /// Set the capacity (in bytes) of the internal buffer used in the CSV writer.
    /// This defaults to a reasonable setting.
    #[serde(default = "default_capacity")]
    pub capacity: usize,

    /// Configures the fields that will be encoded, as well as the order in which they
    /// appear in the output.
    ///
    /// If a field is not present in the event, the output will be an empty string.
    ///
    /// Values of type `Array`, `Object`, and `Regex` are not supported and the
    /// output will be an empty string.
    pub fields: Vec<ConfigTargetPath>,
}

const fn default_delimiter() -> u8 {
    b','
}

const fn default_escape() -> u8 {
    b'"'
}

const fn default_double_quote() -> bool {
    true
}

const fn default_capacity() -> usize {
    8 * (1 << 10)
}

impl Default for CsvSerializerOptions {
    fn default() -> Self {
        Self {
            delimiter: default_delimiter(),
            double_quote: default_double_quote(),
            escape: default_escape(),
            quote: default_escape(),
            quote_style: QuoteStyle::default(),
            capacity: default_capacity(),
            fields: Vec::new(),
        }
    }
}

impl CsvSerializerOptions {
    fn csv_quote_style(&self) -> csv_core::QuoteStyle {
        match self.quote_style {
            QuoteStyle::Always => csv_core::QuoteStyle::Always,
            QuoteStyle::Necessary => csv_core::QuoteStyle::Necessary,
            QuoteStyle::NonNumeric => csv_core::QuoteStyle::NonNumeric,
            QuoteStyle::Never => csv_core::QuoteStyle::Never,
        }
    }
}

/// Serializer that converts an `Event` to bytes using the CSV format.
#[derive(Debug, Clone)]
pub struct CsvSerializer {
    wtr: csv_core::Writer,
    fields: Vec<ConfigTargetPath>,
    internal_buffer: Vec<u8>,
}

impl CsvSerializer {
    /// Creates a new `CsvSerializer`.
    pub fn new(config: CsvSerializerConfig) -> Self {
        // 'flexible' is not needed since every event is a single context free csv line
        let wtr = csv_core::WriterBuilder::new()
            .delimiter(config.csv.delimiter)
            .double_quote(config.csv.double_quote)
            .escape(config.csv.escape)
            .quote_style(config.csv.csv_quote_style())
            .quote(config.csv.quote)
            .build();

        let internal_buffer = if config.csv.capacity < 1 {
            vec![0; 1]
        } else {
            vec![0; config.csv.capacity]
        };

        Self {
            wtr,
            internal_buffer,
            fields: config.csv.fields,
        }
    }
}

impl Encoder<Event> for CsvSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();

        let mut used_buffer_bytes = 0;
        for (fields_written, field) in self.fields.iter().enumerate() {
            let field_value = log.get(field);

            // write field delimiter
            if fields_written > 0 {
                loop {
                    let (res, bytes_written) = self
                        .wtr
                        .delimiter(&mut self.internal_buffer[used_buffer_bytes..]);
                    used_buffer_bytes += bytes_written;
                    match res {
                        csv_core::WriteResult::InputEmpty => {
                            break;
                        }
                        csv_core::WriteResult::OutputFull => {
                            buffer.extend_from_slice(&self.internal_buffer[..used_buffer_bytes]);
                            used_buffer_bytes = 0;
                        }
                    }
                }
            }

            // get string value of current field
            let field_value = match field_value {
                Some(Value::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                Some(Value::Integer(int)) => int.to_string(),
                Some(Value::Float(float)) => float.to_string(),
                Some(Value::Boolean(bool)) => bool.to_string(),
                Some(Value::Timestamp(timestamp)) => {
                    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
                }
                Some(Value::Null) => String::new(),
                // Other value types: Array, Regex, Object are not supported by the CSV format.
                Some(_) => String::new(),
                None => String::new(),
            };

            // mutable byte_slice so it can be written in chunks of buffer fills up
            let mut field_value = field_value.as_bytes();
            // write field_value to internal buffer
            loop {
                let (res, bytes_read, bytes_written) = self
                    .wtr
                    .field(field_value, &mut self.internal_buffer[used_buffer_bytes..]);

                field_value = &field_value[bytes_read..];
                used_buffer_bytes += bytes_written;

                match res {
                    csv_core::WriteResult::InputEmpty => break,
                    csv_core::WriteResult::OutputFull => {
                        buffer.extend_from_slice(&self.internal_buffer[..used_buffer_bytes]);
                        used_buffer_bytes = 0;
                    }
                }
            }
        }

        // finish current event (potentially add closing quotes)
        loop {
            let (res, bytes_written) = self
                .wtr
                .finish(&mut self.internal_buffer[used_buffer_bytes..]);
            used_buffer_bytes += bytes_written;
            match res {
                csv_core::WriteResult::InputEmpty => break,
                csv_core::WriteResult::OutputFull => {
                    buffer.extend_from_slice(&self.internal_buffer[..used_buffer_bytes]);
                    used_buffer_bytes = 0;
                }
            }
        }

        if used_buffer_bytes > 0 {
            buffer.extend_from_slice(&self.internal_buffer[..used_buffer_bytes]);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use chrono::DateTime;
    use ordered_float::NotNan;
    use vector_common::btreemap;
    use vector_core::event::{LogEvent, Value};

    use super::*;

    #[test]
    fn build_error_on_empty_fields() {
        let opts = CsvSerializerOptions::default();
        let config = CsvSerializerConfig::new(opts);
        let err = config.build().unwrap_err();
        assert_eq!(err.to_string(), "At least one CSV field must be specified");
    }

    #[test]
    fn serialize_fields() {
        let event = Event::Log(LogEvent::from(btreemap! {
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
        let fields = vec![
            ConfigTargetPath::try_from("foo".to_string()).unwrap(),
            ConfigTargetPath::try_from("int".to_string()).unwrap(),
            ConfigTargetPath::try_from("comma".to_string()).unwrap(),
            ConfigTargetPath::try_from("float".to_string()).unwrap(),
            ConfigTargetPath::try_from("missing".to_string()).unwrap(),
            ConfigTargetPath::try_from("space".to_string()).unwrap(),
            ConfigTargetPath::try_from("time".to_string()).unwrap(),
            ConfigTargetPath::try_from("quote".to_string()).unwrap(),
            ConfigTargetPath::try_from("bool".to_string()).unwrap(),
        ];

        let opts = CsvSerializerOptions {
            fields,
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            b"bar,123,\"abc,bcd\",3.1415925,,sp ace,2023-02-27T07:04:49.363Z,\"the \"\"quote\"\" should be escaped\",true".as_slice()
        );
    }

    #[test]
    fn serialize_order() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("value1"),
            "field2" => Value::from("value2"),
            "field3" => Value::from("value3"),
            "field4" => Value::from("value4"),
            "field5" => Value::from("value5"),
        }));
        let fields = vec![
            ConfigTargetPath::try_from("field1".to_string()).unwrap(),
            ConfigTargetPath::try_from("field5".to_string()).unwrap(),
            ConfigTargetPath::try_from("field5".to_string()).unwrap(),
            ConfigTargetPath::try_from("field3".to_string()).unwrap(),
            ConfigTargetPath::try_from("field2".to_string()).unwrap(),
        ];
        let opts = CsvSerializerOptions {
            fields,
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            b"value1,value5,value5,value3,value2".as_slice()
        );
    }

    #[test]
    fn correct_quoting() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from(1),
            "field2" => Value::from("foo\"bar"),
        }));
        let fields = vec![
            ConfigTargetPath::try_from("field1".to_string()).unwrap(),
            ConfigTargetPath::try_from("field2".to_string()).unwrap(),
        ];
        let opts = CsvSerializerOptions {
            fields,
            quote_style: QuoteStyle::Always,
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"\"1\",\"foo\"\"bar\"".as_slice());
    }

    #[test]
    fn custom_delimiter() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("value1"),
            "field2" => Value::from("value2"),
        }));
        let fields = vec![
            ConfigTargetPath::try_from("field1".to_string()).unwrap(),
            ConfigTargetPath::try_from("field2".to_string()).unwrap(),
        ];
        let opts = CsvSerializerOptions {
            fields,
            delimiter: b'\t',
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"value1\tvalue2".as_slice());
    }

    #[test]
    fn custom_escape_char() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("foo\"bar"),
        }));
        let fields = vec![ConfigTargetPath::try_from("field1".to_string()).unwrap()];
        let opts = CsvSerializerOptions {
            fields,
            double_quote: false,
            escape: b'\\',
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"\"foo\\\"bar\"".as_slice());
    }

    #[test]
    fn custom_quote_char() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("foo \" $ bar"),
        }));
        let fields = vec![ConfigTargetPath::try_from("field1".to_string()).unwrap()];
        let opts = CsvSerializerOptions {
            fields,
            quote: b'$',
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"$foo \" $$ bar$".as_slice());
    }

    #[test]
    fn custom_quote_style() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("foo\"bar"),
        }));
        let fields = vec![ConfigTargetPath::try_from("field1".to_string()).unwrap()];
        let opts = CsvSerializerOptions {
            fields,
            quote_style: QuoteStyle::Never,
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"foo\"bar".as_slice());
    }

    #[test]
    fn more_input_then_capacity() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("foo bar"),
        }));
        let fields = vec![ConfigTargetPath::try_from("field1".to_string()).unwrap()];
        let opts = CsvSerializerOptions {
            fields,
            capacity: 3,
            ..Default::default()
        };
        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), b"foo bar".as_slice());
    }
}

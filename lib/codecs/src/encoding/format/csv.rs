use crate::encoding::BuildError;
use bytes::{BufMut, BytesMut};
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
            let opts = CsvSerializerOptions {
                delimiter: self.csv.delimiter,
                escape: self.csv.escape,
                double_quote: self.csv.double_quote,
                fields: self.csv.fields.clone(),
            };
            let config = CsvSerializerConfig::new(opts);

            Ok(CsvSerializer::new(config))
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

/// Config used to build a `CsvSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct CsvSerializerOptions {
    /// The field delimiter to use when writing CSV.
    pub delimiter: u8,

    /// Enable double quote escapes.
    ///
    /// This is enabled by default, but it may be disabled. When disabled, quotes in
    /// field data are escaped instead of doubled.
    pub double_quote: bool,

    /// The escape character to use when writing CSV.
    ///
    /// In some variants of CSV, quotes are escaped using a special escape character
    /// like \ (instead of escaping quotes by doubling them).
    ///
    /// To use this `double_uotes` needs to be disabled as well
    pub escape: u8,

    /// Configures the fields that will be encoded, as well as the order in which they
    /// appear in the output.
    ///
    /// If a field is not present in the event, the output will be an empty string.
    ///
    /// Values of type `Array`, `Object`, and `Regex` are not supported and the
    /// output will be an empty string.
    pub fields: Vec<ConfigTargetPath>,
}

impl Default for CsvSerializerOptions {
    fn default() -> CsvSerializerOptions {
        CsvSerializerOptions {
            delimiter: b',',
            double_quote: true,
            escape: b'"',
            fields: vec![]
        }
    }
}

/// Serializer that converts an `Event` to bytes using the CSV format.
#[derive(Debug, Clone)]
pub struct CsvSerializer {
    delimiter: u8,
    double_quote: bool,
    escape: u8,
    fields: Vec<ConfigTargetPath>,
}

impl CsvSerializer {
    /// Creates a new `CsvSerializer`.
    pub fn new(conf: CsvSerializerConfig) -> Self {
        Self {
            delimiter: conf.csv.delimiter,
            double_quote: conf.csv.double_quote,
            escape: conf.csv.escape,
            fields: conf.csv.fields,
        }
    }
}

impl Encoder<Event> for CsvSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();
        let mut wtr = csv::WriterBuilder::new()
            .delimiter(self.delimiter)
            .double_quote(self.double_quote)
            .escape(self.escape)
            .terminator(csv::Terminator::Any(b'\0')) // TODO: this needs proper 'nothig' value
            .from_writer(buffer.writer());

        for field in &self.fields {
            match log.get(field) {
                Some(Value::Bytes(bytes)) => {
                    wtr.write_field(String::from_utf8_lossy(bytes).to_string())?
                }
                Some(Value::Integer(int)) => wtr.write_field(int.to_string())?,
                Some(Value::Float(float)) => wtr.write_field(float.to_string())?,
                Some(Value::Boolean(bool)) => wtr.write_field(bool.to_string())?,
                Some(Value::Timestamp(timestamp)) => {
                    wtr.write_field(timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true))?
                }
                Some(Value::Null) => wtr.write_field("")?,
                // Other value types: Array, Regex, Object are not supported by the CSV format.
                Some(_) => wtr.write_field("")?,
                None => wtr.write_field("")?,
            }
        }
        wtr.flush()?;
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

        let mut opts = CsvSerializerOptions::default();
        opts.fields = fields;

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
        let mut opts = CsvSerializerOptions::default();
        opts.fields = fields;

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
            "field1" => Value::from("value1 \" value2"),
        }));
        let fields = vec![
            ConfigTargetPath::try_from("field1".to_string()).unwrap(),
        ];
        let mut opts = CsvSerializerOptions::default();
        opts.fields = fields;

        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();
        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            b"\"value1 \"\" value2\"".as_slice()
        );
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
        let mut opts = CsvSerializerOptions::default();
        opts.fields = fields;
        opts.delimiter = b'\t';

        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();
        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            b"value1\tvalue2".as_slice()
        );
    }

    #[test]
    fn custom_escape_char() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "field1" => Value::from("hallo world"),
        }));
        let fields = vec![
            ConfigTargetPath::try_from("field1".to_string()).unwrap(),
        ];
        let mut opts = CsvSerializerOptions::default();
        opts.fields = fields;
        opts.delimiter = b' ';
        opts.double_quote = true;
        //opts.escape = b'\'';

        let config = CsvSerializerConfig::new(opts);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();
        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            &bytes.freeze()[..],
            b"\"hallo\\\"world\"".as_slice()
        );
    }

}

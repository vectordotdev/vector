use crate::encoding::BuildError;
use bytes::{BufMut, BytesMut};
use lookup::lookup_v2::OwnedValuePath;
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};

/// Config used to build a `CsvSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone, Default)]
pub struct CsvSerializerConfig {
    /// The CSV fields.
    pub fields: Vec<OwnedValuePath>,
}

impl CsvSerializerConfig {
    /// Creates a new `CsvSerializerConfig`.
    pub const fn new(fields: Vec<OwnedValuePath>) -> Self {
        Self { fields }
    }

    /// Build the `CsvSerializer` from this configuration.
    pub fn build(&self) -> Result<CsvSerializer, BuildError> {
        if self.fields.is_empty() {
            Err("At least one CSV field must be specified".into())
        } else {
            Ok(CsvSerializer::new(self.fields.clone()))
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

/// Serializer that converts an `Event` to bytes using the CSV format.
#[derive(Debug, Clone)]
pub struct CsvSerializer {
    fields: Vec<OwnedValuePath>,
}

impl CsvSerializer {
    /// Creates a new `CsvSerializer`.
    pub const fn new(fields: Vec<OwnedValuePath>) -> Self {
        Self { fields }
    }
}

impl Encoder<Event> for CsvSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let log = event.into_log();
        let values = log.value();
        let mut wtr = csv::Writer::from_writer(vec![]);

        for field in &self.fields {
            match values.get(field) {
                Some(Value::Bytes(bytes)) => wtr.write_field(String::from_utf8(bytes.to_vec())?)?,
                Some(Value::Integer(int)) => wtr.write_field(int.to_string())?,
                Some(Value::Float(float)) => wtr.write_field(float.to_string())?,
                Some(Value::Boolean(bool)) => wtr.write_field(bool.to_string())?,
                Some(Value::Timestamp(timestamp)) => wtr.write_field(timestamp.to_rfc3339())?,
                Some(Value::Null) => wtr.write_field("NaN")?,
                // Other value types: Array, Regex, Object are not supported by the CSV format.
                Some(_) => wtr.write_field("NaN")?,
                None => wtr.write_field("NaN")?,
            }
        }

        buffer.put_slice(wtr.into_inner()?.as_ref());
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
        let config = CsvSerializerConfig::new(vec![]);
        let err = config.build().unwrap_err();
        assert_eq!(err.to_string(), "At least one csv field must be specified");
    }

    #[test]
    fn serialize_csv() {
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
            OwnedValuePath::try_from("foo".to_string()).unwrap(),
            OwnedValuePath::try_from("int".to_string()).unwrap(),
            OwnedValuePath::try_from("comma".to_string()).unwrap(),
            OwnedValuePath::try_from("float".to_string()).unwrap(),
            OwnedValuePath::try_from("missing".to_string()).unwrap(),
            OwnedValuePath::try_from("space".to_string()).unwrap(),
            OwnedValuePath::try_from("time".to_string()).unwrap(),
            OwnedValuePath::try_from("quote".to_string()).unwrap(),
            OwnedValuePath::try_from("bool".to_string()).unwrap(),
        ];
        let config = CsvSerializerConfig::new(fields);
        let mut serializer = config.build().unwrap();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            b"bar,123,\"abc,bcd\",3.1415925,NaN,sp ace,2023-02-27T07:04:49.363+00:00,\"the \"\"quote\"\" should be escaped\",true".as_slice()
        );
    }
}

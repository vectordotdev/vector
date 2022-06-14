use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

/// Config used to build a `NativeJsonSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeJsonSerializerConfig;

impl NativeJsonSerializerConfig {
    /// Build the `NativeJsonSerializer` from this configuration.
    pub const fn build(&self) -> NativeJsonSerializer {
        NativeJsonSerializer
    }

    /// The data type of events that are accepted by `NativeJsonSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct NativeJsonSerializer;

impl NativeJsonSerializer {
    /// Creates a new `NativeJsonSerializer`.
    pub const fn new() -> Self {
        Self
    }

    /// Encode event and represent it as native JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(&event)
    }
}

impl Encoder<Event> for NativeJsonSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        serde_json::to_writer(writer, &event).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use vector_common::btreemap;
    use vector_core::event::Value;

    use super::*;

    #[test]
    fn serialize_json() {
        let event = Event::from(btreemap! {
            "foo" => Value::from("bar")
        });
        let mut serializer = NativeJsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), r#"{"log":{"foo":"bar"}}"#);
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::from(btreemap! {
            "foo" => Value::from("bar")
        });
        let mut serializer = NativeJsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }
}

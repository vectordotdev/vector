use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

/// Config used to build a `JsonSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonSerializerConfig;

impl JsonSerializerConfig {
    /// Creates a new `JsonSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `JsonSerializer` from this configuration.
    pub const fn build(&self) -> JsonSerializer {
        JsonSerializer
    }

    /// The data type of events that are accepted by `JsonSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // JSON, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct JsonSerializer;

impl JsonSerializer {
    /// Creates a new `JsonSerializer`.
    pub const fn new() -> Self {
        Self
    }

    /// Encode event and represent it as JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, serde_json::Error> {
        match event {
            Event::Log(log) => serde_json::to_value(&log),
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(&trace),
        }
    }
}

impl Encoder<Event> for JsonSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        match event {
            Event::Log(log) => serde_json::to_writer(writer, &log),
            Event::Metric(metric) => serde_json::to_writer(writer, &metric),
            Event::Trace(trace) => serde_json::to_writer(writer, &trace),
        }
        .map_err(Into::into)
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
        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), r#"{"foo":"bar"}"#);
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::from(btreemap! {
            "foo" => Value::from("bar")
        });
        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }
}

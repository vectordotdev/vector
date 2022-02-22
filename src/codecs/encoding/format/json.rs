use bytes::{BufMut, BytesMut};
use lookup::LookupBuf;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use value::Kind;

use crate::{event::Event, schema};

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

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // Technically we can serialize any type of `Value` to JSON, even "non-JSON" types such as
        // `timestamp`, but it's not a lossless serialization. Should we allow it in the schema?
        schema::Requirement::empty().require_field(&LookupBuf::root(), Kind::json())
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
}

impl Encoder<Event> for JsonSerializer {
    type Error = crate::Error;

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
    use super::*;
    use crate::event::Value;
    use bytes::BytesMut;
    use vector_common::btreemap;

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
}

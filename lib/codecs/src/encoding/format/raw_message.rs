use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    schema,
};
use vrl::value::Kind;

/// Config used to build a `RawMessageSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawMessageSerializerConfig;

impl RawMessageSerializerConfig {
    /// Creates a new `RawMessageSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `RawMessageSerializer` from this configuration.
    pub const fn build(&self) -> RawMessageSerializer {
        RawMessageSerializer
    }

    /// The data type of events that are accepted by `RawMessageSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty().required_meaning(log_schema().message_key(), Kind::any())
    }
}

/// Serializer that converts an `Event` to bytes by extracting the message key.
#[derive(Debug, Clone)]
pub struct RawMessageSerializer;

impl RawMessageSerializer {
    /// Creates a new `RawMessageSerializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Encoder<Event> for RawMessageSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let message_key = log_schema().message_key();

        let log = event.as_log();

        if let Some(bytes) = log
            .get_by_meaning(message_key)
            .or_else(|| log.get(message_key))
            .map(|value| value.coerce_to_bytes())
        {
            buffer.put(bytes);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use vector_core::event::LogEvent;

    use super::*;

    #[test]
    fn serialize_bytes() {
        let input = Event::from(LogEvent::from_str_legacy("foo"));
        let mut serializer = RawMessageSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }
}

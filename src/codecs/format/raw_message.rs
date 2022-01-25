use crate::{
    codecs::encoding::{BoxedSerializer, Serializer, SerializerConfig},
    config::log_schema,
    event::Event,
};
use bytes::BufMut;
use serde::{Deserialize, Serialize};

/// Config used to build a `RawMessageSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawMessageSerializerConfig;

impl RawMessageSerializerConfig {
    /// Creates a new `RawMessageSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "text")]
impl SerializerConfig for RawMessageSerializerConfig {
    fn build(&self) -> crate::Result<BoxedSerializer> {
        Ok(Box::new(RawMessageSerializer))
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

impl Serializer for RawMessageSerializer {
    fn serialize(&self, event: Event, buffer: &mut bytes::BytesMut) -> crate::Result<()> {
        let bytes = match event {
            Event::Log(log) => log
                .get(log_schema().message_key())
                .map(|value| value.as_bytes()),
            Event::Metric(_) => None,
        };

        if let Some(bytes) = bytes {
            buffer.put(bytes);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{Bytes, BytesMut};

    #[test]
    fn serialize_bytes() {
        let input = Event::from("foo");
        let serializer = RawMessageSerializer;

        let mut buffer = BytesMut::new();
        serializer.serialize(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }
}

use crate::{
    codecs::encoding::{BoxedSerializer, SerializerConfig},
    config::log_schema,
    event::Event,
};
use bytes::BufMut;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;

/// Config used to build a `TextSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TextSerializerConfig;

impl TextSerializerConfig {
    /// Creates a new `TextSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "text")]
impl SerializerConfig for TextSerializerConfig {
    fn build(&self) -> crate::Result<BoxedSerializer> {
        Ok(Box::new(TextSerializer))
    }
}

/// Serializer that converts an `Event` to bytes by extracting the message key.
#[derive(Debug, Clone)]
pub struct TextSerializer;

impl TextSerializer {
    /// Creates a new `TextSerializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Encoder<Event> for TextSerializer {
    type Error = crate::Error;

    fn encode(&mut self, event: Event, buffer: &mut bytes::BytesMut) -> Result<(), Self::Error> {
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
        let mut serializer = TextSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }
}

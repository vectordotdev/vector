use crate::{
    codecs::{
        decoding::{BoxedDeserializer, Deserializer, DeserializerConfig},
        encoding::{BoxedSerializer, SerializerConfig},
    },
    config::log_schema,
    event::{Event, LogEvent},
};
use bytes::{BufMut, Bytes};
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use tokio_util::codec::Encoder;

/// Config used to build a `BytesDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDeserializerConfig;

impl BytesDeserializerConfig {
    /// Creates a new `BytesDeserializerConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "bytes")]
impl DeserializerConfig for BytesDeserializerConfig {
    fn build(&self) -> crate::Result<BoxedDeserializer> {
        Ok(Box::new(BytesDeserializer))
    }
}

/// Deserializer that converts bytes to an `Event`.
///
/// This deserializer can be considered as the no-op action for input where no
/// further decoding has been specified.
#[derive(Debug, Clone)]
pub struct BytesDeserializer;

impl BytesDeserializer {
    /// Creates a new `BytesDeserializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Deserializer for BytesDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let mut log = LogEvent::default();
        log.insert(log_schema().message_key(), bytes);
        Ok(smallvec![log.into()])
    }
}

/// Config used to build a `BytesSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesSerializerConfig;

impl BytesSerializerConfig {
    /// Creates a new `BytesSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "bytes")]
impl SerializerConfig for BytesSerializerConfig {
    fn build(&self) -> crate::Result<BoxedSerializer> {
        Ok(Box::new(BytesSerializer))
    }
}

/// Serializer that converts an `Event` to bytes.
///
/// This serializer can be considered as the no-op action for input where no
/// further encoding has been specified.
#[derive(Debug, Clone)]
pub struct BytesSerializer;

impl BytesSerializer {
    /// Creates a new `BytesSerializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Encoder<Event> for BytesSerializer {
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
    use crate::config::log_schema;
    use bytes::BytesMut;

    #[test]
    fn deserialize_bytes() {
        let input = Bytes::from("foo");
        let deserializer = BytesDeserializer;

        let events = deserializer.parse(input).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "foo".into());
        }

        assert_eq!(events.next(), None);
    }

    #[test]
    fn serialize_bytes() {
        let input = Event::from("foo");
        let mut serializer = BytesSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }
}

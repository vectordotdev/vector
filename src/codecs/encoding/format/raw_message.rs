use crate::{config::log_schema, event::Event};

use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;

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
    type Error = crate::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
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
        let mut serializer = RawMessageSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }
}

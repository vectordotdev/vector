use crate::{
    codecs::decoding::{BoxedDeserializer, Deserializer, DeserializerConfig},
    config::log_schema,
    event::{Event, LogEvent},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;

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
}

use bytes::Bytes;
use lookup::LookupBuf;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use value::Kind;
use vector_core::config::LogNamespace;
use vector_core::{
    config::{log_schema, DataType},
    event::{Event, LogEvent},
    schema,
};

use super::Deserializer;

/// Config used to build a `BytesDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDeserializerConfig;

impl BytesDeserializerConfig {
    /// Creates a new `BytesDeserializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `BytesDeserializer` from this configuration.
    pub fn build(&self) -> BytesDeserializer {
        BytesDeserializer::new()
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => schema::Definition::legacy_empty().with_field(
                log_schema().message_key(),
                Kind::bytes(),
                Some("message"),
            ),
            LogNamespace::Vector => {
                schema::Definition::empty_with_kind(Kind::bytes(), [log_namespace])
                    .with_known_meaning(LookupBuf::root(), "message")
            }
        }
    }
}

/// Deserializer that converts bytes to an `Event`.
///
/// This deserializer can be considered as the no-op action for input where no
/// further decoding has been specified.
#[derive(Debug, Clone)]
pub struct BytesDeserializer {
    // Only used with the "Legacy" namespace
    log_schema_message_key: &'static str,
}

impl Default for BytesDeserializer {
    fn default() -> Self {
        Self::new()
    }
}

impl BytesDeserializer {
    /// Creates a new `BytesDeserializer`.
    pub fn new() -> Self {
        Self {
            log_schema_message_key: log_schema().message_key(),
        }
    }
}

impl Deserializer for BytesDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_core::Result<SmallVec<[Event; 1]>> {
        let log = match log_namespace {
            LogNamespace::Vector => log_namespace.new_log_from_data(bytes),
            LogNamespace::Legacy => {
                let mut log = LogEvent::default();
                log.insert(self.log_schema_message_key, bytes);
                log
            }
        };
        Ok(smallvec![log.into()])
    }
}

#[cfg(test)]
mod tests {
    use value::Value;
    use vector_core::config::log_schema;

    use super::*;

    #[test]
    fn deserialize_bytes_legacy_namespace() {
        let input = Bytes::from("foo");
        let deserializer = BytesDeserializer::new();

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "foo".into());
        }

        assert_eq!(events.next(), None);
    }

    #[test]
    fn deserialize_bytes_vector_namespace() {
        let input = Bytes::from("foo");
        let deserializer = BytesDeserializer::new();

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        assert_eq!(events.len(), 1);

        assert_eq!(events[0].as_log().get(".").unwrap(), &Value::from("foo"));
    }
}

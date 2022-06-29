use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use value::Kind;
use vector_core::{
    config::{log_schema, DataType},
    event::Event,
    schema,
};

/// Config used to build a `TextSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TextSerializerConfig;

impl TextSerializerConfig {
    /// Creates a new `TextSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `TextSerializer` from this configuration.
    pub const fn build(&self) -> TextSerializer {
        TextSerializer
    }

    /// The data type of events that are accepted by `TextSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log | DataType::Metric
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty().required_meaning(log_schema().message_key(), Kind::any())
    }
}

/// Serializer that converts a log to bytes by extracting the message key, or converts a metric
/// to bytes by calling its `Display` implementation.
///
/// This serializer exists to emulate the behavior of the `StandardEncoding::Text` for backwards
/// compatibility, until it is phased out completely.
#[derive(Debug, Clone)]
pub struct TextSerializer;

impl TextSerializer {
    /// Creates a new `TextSerializer`.
    pub const fn new() -> Self {
        Self
    }
}

impl Encoder<Event> for TextSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let message_key = log_schema().message_key();

        match event {
            Event::Log(log) => {
                if let Some(bytes) = log
                    .get_by_meaning(message_key)
                    .or_else(|| log.get(message_key))
                    .map(|value| value.coerce_to_bytes())
                {
                    buffer.put(bytes);
                }
            }
            Event::Metric(metric) => {
                let bytes = metric.to_string();
                buffer.put(bytes.as_ref());
            }
            Event::Trace(_) => {}
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use vector_core::event::{Metric, MetricKind, MetricValue};

    use super::*;

    #[test]
    fn serialize_bytes_event() {
        let input = Event::from("foo");
        let mut serializer = TextSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("foo"));
    }

    #[test]
    fn serialize_bytes_metric() {
        let input = Event::Metric(Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));
        let mut serializer = TextSerializer;

        let mut buffer = BytesMut::new();
        serializer.encode(input, &mut buffer).unwrap();

        assert_eq!(buffer.freeze(), Bytes::from("users{} + bob"));
    }
}

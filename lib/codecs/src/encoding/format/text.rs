use crate::encoding::format::common::get_serializer_schema_requirement;
use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

use crate::MetricTagValues;

/// Config used to build a `TextSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone, Default)]
pub struct TextSerializerConfig {
    /// Controls how metric tag values are encoded.
    ///
    /// When set to `single`, only the last non-bare value of tags are displayed with the
    /// metric.  When set to `full`, all metric tags are exposed as separate assignments.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub metric_tag_values: MetricTagValues,
}

impl TextSerializerConfig {
    /// Creates a new `TextSerializerConfig`.
    pub const fn new(metric_tag_values: MetricTagValues) -> Self {
        Self { metric_tag_values }
    }

    /// Build the `TextSerializer` from this configuration.
    pub const fn build(&self) -> TextSerializer {
        TextSerializer::new(self.metric_tag_values)
    }

    /// The data type of events that are accepted by `TextSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log | DataType::Metric
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        get_serializer_schema_requirement()
    }
}

/// Serializer that converts a log to bytes by extracting the message key, or converts a metric
/// to bytes by calling its `Display` implementation.
///
/// This serializer exists to emulate the behavior of the `StandardEncoding::Text` for backwards
/// compatibility, until it is phased out completely.
#[derive(Debug, Clone)]
pub struct TextSerializer {
    metric_tag_values: MetricTagValues,
}

impl TextSerializer {
    /// Creates a new `TextSerializer`.
    pub const fn new(metric_tag_values: MetricTagValues) -> Self {
        Self { metric_tag_values }
    }
}

impl Encoder<Event> for TextSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        match event {
            Event::Log(log) => {
                if let Some(bytes) = log.get_message().map(|value| value.coerce_to_bytes()) {
                    buffer.put(bytes);
                }
            }
            Event::Metric(mut metric) => {
                if self.metric_tag_values == MetricTagValues::Single {
                    metric.reduce_tags_to_single();
                }
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
    use vector_core::event::{LogEvent, Metric, MetricKind, MetricValue};
    use vector_core::metric_tags;

    use super::*;

    #[test]
    fn serialize_log() {
        let buffer = serialize(
            TextSerializerConfig::default(),
            Event::from(LogEvent::from_str_legacy("foo")),
        );
        assert_eq!(buffer, Bytes::from("foo"));
    }

    #[test]
    fn serialize_metric() {
        let buffer = serialize(
            TextSerializerConfig::default(),
            Event::Metric(Metric::new(
                "users",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["bob".into()].into_iter().collect(),
                },
            )),
        );
        assert_eq!(buffer, Bytes::from("users{} + bob"));
    }

    #[test]
    fn serialize_metric_tags_full() {
        let buffer = serialize(
            TextSerializerConfig {
                metric_tag_values: MetricTagValues::Full,
            },
            metric2(),
        );
        assert_eq!(
            buffer,
            Bytes::from(r#"counter{a="first",a,a="second"} + 1"#)
        );
    }

    #[test]
    fn serialize_metric_tags_single() {
        let buffer = serialize(
            TextSerializerConfig {
                metric_tag_values: MetricTagValues::Single,
            },
            metric2(),
        );
        assert_eq!(buffer, Bytes::from(r#"counter{a="second"} + 1"#));
    }

    fn metric2() -> Event {
        Event::Metric(
            Metric::new(
                "counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(metric_tags! (
                "a" => "first",
                "a" => None,
                "a" => "second",
            ))),
        )
    }

    fn serialize(config: TextSerializerConfig, input: Event) -> Bytes {
        let mut buffer = BytesMut::new();
        config.build().encode(input, &mut buffer).unwrap();
        buffer.freeze()
    }
}

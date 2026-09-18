use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

use crate::native_json::to_json_value;

/// Config used to build a `NativeJsonSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NativeJsonSerializerConfig;

impl NativeJsonSerializerConfig {
    /// Build the `NativeJsonSerializer` from this configuration.
    pub const fn build(&self) -> NativeJsonSerializer {
        NativeJsonSerializer
    }

    /// The data type of events that are accepted by `NativeJsonSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct NativeJsonSerializer;

impl NativeJsonSerializer {
    /// Encode event and represent it as native JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, vector_common::Error> {
        to_json_value(event)
    }
}

impl Encoder<Event> for NativeJsonSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        serde_json::to_writer(writer, &to_json_value(event)?).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use uuid::Uuid;
    use vector_core::{
        buckets,
        event::{LogEvent, Metric, MetricKind, MetricValue, TraceEvent, Value},
        metric_tags,
    };
    use vrl::btreemap;

    use super::*;

    fn with_fixed_source_event_id(mut event: Event) -> Event {
        let metadata = std::mem::take(event.metadata_mut());
        *event.metadata_mut() = metadata.with_source_event_id(Some(Uuid::nil()));
        event
    }

    #[test]
    fn serialize_log_is_byte_stable() {
        let event = with_fixed_source_event_id(Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        })));
        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.as_ref(),
            br#"{"event":{"log":{"metadataFull":{"sourceEventId":"AAAAAAAAAAAAAAAAAAAAAA==","value":{"map":{}}},"value":{"map":{"fields":{"foo":{"rawBytes":"YmFy"}}}}}}}"#
        );
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }

    #[test]
    fn serialize_aggregated_histogram() {
        let histogram_event = with_fixed_source_event_id(Event::from(
            Metric::new(
                "histogram",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    count: 1,
                    sum: 1.0,
                    buckets: buckets!(f64::NEG_INFINITY => 0 ,2.0 => 1, f64::INFINITY => 0),
                },
            )
            .with_tags(Some(metric_tags!("service" => "api"))),
        ));

        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();
        serializer.encode(histogram_event, &mut bytes).unwrap();
        assert_eq!(
            bytes.as_ref(),
            br#"{"event":{"metric":{"aggregatedHistogram3":{"buckets":[{"upperLimit":"-Infinity"},{"count":"1","upperLimit":2.0},{"upperLimit":"Infinity"}],"count":"1","sum":1.0},"kind":"Absolute","metadataFull":{"sourceEventId":"AAAAAAAAAAAAAAAAAAAAAA==","value":{"map":{}}},"name":"histogram","tagsV2":{"service":{"values":[{"value":"api"}]}}}}}"#
        );
    }

    #[test]
    fn serialize_trace_is_byte_stable() {
        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();

        serializer
            .encode(
                with_fixed_source_event_id(Event::Trace(TraceEvent::from(btreemap! {
                    "foo" => Value::from("bar")
                }))),
                &mut bytes,
            )
            .unwrap();

        assert_eq!(
            bytes.as_ref(),
            br#"{"event":{"trace":{"fields":{"foo":{"rawBytes":"YmFy"}},"metadataFull":{"sourceEventId":"AAAAAAAAAAAAAAAAAAAAAA==","value":{"map":{}}}}}}"#
        );
    }
}

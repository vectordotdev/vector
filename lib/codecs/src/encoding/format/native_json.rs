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
    use vector_core::{
        buckets,
        event::{LogEvent, Metric, MetricKind, MetricValue, TraceEvent, Value},
        metric_tags,
    };
    use vrl::btreemap;

    use super::*;

    #[test]
    fn serialize_json() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json.pointer("/log/value/map/fields/foo/rawBytes"),
            Some(&serde_json::Value::String("YmFy".to_owned()))
        );
        assert!(json.pointer("/log/fields").is_none());
        assert!(json.pointer("/log/metadata").is_none());
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
        let histogram_event = Event::from(
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
        );

        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();
        serializer
            .encode(histogram_event.clone(), &mut bytes)
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json.pointer("/metric/aggregatedHistogram3/count"),
            Some(&serde_json::Value::String("1".to_owned()))
        );
        assert!(json.pointer("/metric/aggregatedHistogram1").is_none());
        assert!(json.pointer("/metric/aggregatedHistogram2").is_none());
        assert!(json.pointer("/metric/tagsV1").is_none());
        assert_eq!(
            json.pointer("/metric/tagsV2/service/values/0/value"),
            Some(&serde_json::Value::String("api".to_owned()))
        );
        assert!(json.pointer("/metric/metadata").is_none());
    }

    #[test]
    fn serialize_trace_omits_deprecated_metadata() {
        let mut serializer = NativeJsonSerializer;
        let mut bytes = BytesMut::new();

        serializer
            .encode(Event::Trace(TraceEvent::default()), &mut bytes)
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.pointer("/trace/metadata").is_none());
        assert!(json.pointer("/trace/metadataFull").is_some());
    }
}

use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

/// Config used to build a `JsonSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JsonSerializerConfig;

impl JsonSerializerConfig {
    /// Creates a new `JsonSerializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `JsonSerializer` from this configuration.
    pub const fn build(&self) -> JsonSerializer {
        JsonSerializer
    }

    /// The data type of events that are accepted by `JsonSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::all()
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // JSON, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the JSON format.
#[derive(Debug, Clone)]
pub struct JsonSerializer;

impl JsonSerializer {
    /// Creates a new `JsonSerializer`.
    pub const fn new() -> Self {
        Self
    }

    /// Encode event and represent it as JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, vector_core::Error> {
        match event {
            Event::Log(log) => serde_json::to_value(&log),
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(&trace),
        }
        .map_err(|e| e.to_string().into())
    }
}

impl Encoder<Event> for JsonSerializer {
    type Error = vector_core::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        match event {
            Event::Log(log) => serde_json::to_writer(writer, &log),
            Event::Metric(metric) => serde_json::to_writer(writer, &metric),
            Event::Trace(trace) => serde_json::to_writer(writer, &trace),
        }
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use chrono::{TimeZone, Utc};
    use vector_common::btreemap;
    use vector_core::event::{LogEvent, Metric, MetricKind, MetricValue, StatisticKind, Value};

    use super::*;

    #[test]
    fn serialize_json_log() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "x" => Value::from("23"),
            "z" => Value::from(25),
            "a" => Value::from("0"),
        }));
        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(bytes.freeze(), r#"{"a":"0","x":"23","z":25}"#);
    }

    #[test]
    fn serialize_json_metric_counter() {
        let event = Event::Metric(
            Metric::new(
                "foos",
                MetricKind::Incremental,
                MetricValue::Counter { value: 100.0 },
            )
            .with_namespace(Some("vector"))
            .with_tags(Some(
                vec![
                    ("key2".to_owned(), "value2".to_owned()),
                    ("key1".to_owned(), "value1".to_owned()),
                    ("Key3".to_owned(), "Value3".to_owned()),
                ]
                .into_iter()
                .collect(),
            ))
            .with_timestamp(Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11))),
        );

        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            r#"{"name":"foos","namespace":"vector","tags":{"Key3":"Value3","key1":"value1","key2":"value2"},"timestamp":"2018-11-14T08:09:10.000000011Z","kind":"incremental","counter":{"value":100.0}}"#
        );
    }

    #[test]
    fn serialize_json_metric_set() {
        let event = Event::Metric(Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));

        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            r#"{"name":"users","kind":"incremental","set":{"values":["bob"]}}"#
        );
    }

    #[test]
    fn serialize_json_metric_histogram_without_timestamp() {
        let event = Event::Metric(Metric::new(
            "glork",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![10.0 => 1],
                statistic: StatisticKind::Histogram,
            },
        ));

        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event, &mut bytes).unwrap();

        assert_eq!(
            bytes.freeze(),
            r#"{"name":"glork","kind":"incremental","distribution":{"samples":[{"value":10.0,"rate":1}],"statistic":"histogram"}}"#
        );
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let mut serializer = JsonSerializer::new();
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }
}

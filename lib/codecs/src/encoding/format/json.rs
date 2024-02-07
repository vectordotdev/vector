use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::{config::DataType, event::Event, schema};

use crate::MetricTagValues;

/// Config used to build a `JsonSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone, Default)]
pub struct JsonSerializerConfig {
    /// Controls how metric tag values are encoded.
    ///
    /// When set to `single`, only the last non-bare value of tags are displayed with the
    /// metric.  When set to `full`, all metric tags are exposed as separate assignments.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub metric_tag_values: MetricTagValues,
}

impl JsonSerializerConfig {
    /// Creates a new `JsonSerializerConfig`.
    pub const fn new(metric_tag_values: MetricTagValues) -> Self {
        Self { metric_tag_values }
    }

    /// Build the `JsonSerializer` from this configuration.
    pub const fn build(&self) -> JsonSerializer {
        JsonSerializer::new(self.metric_tag_values)
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
pub struct JsonSerializer {
    metric_tag_values: MetricTagValues,
}

impl JsonSerializer {
    /// Creates a new `JsonSerializer`.
    pub const fn new(metric_tag_values: MetricTagValues) -> Self {
        Self { metric_tag_values }
    }

    /// Encode event and represent it as JSON value.
    pub fn to_json_value(&self, event: Event) -> Result<serde_json::Value, vector_common::Error> {
        match event {
            Event::Log(log) => serde_json::to_value(&log),
            Event::Metric(metric) => serde_json::to_value(&metric),
            Event::Trace(trace) => serde_json::to_value(&trace),
        }
        .map_err(|e| e.to_string().into())
    }
}

impl Encoder<Event> for JsonSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let writer = buffer.writer();
        match event {
            Event::Log(log) => serde_json::to_writer(writer, &log),
            Event::Metric(mut metric) => {
                if self.metric_tag_values == MetricTagValues::Single {
                    metric.reduce_tags_to_single();
                }
                serde_json::to_writer(writer, &metric)
            }
            Event::Trace(trace) => serde_json::to_writer(writer, &trace),
        }
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Bytes, BytesMut};
    use chrono::{TimeZone, Timelike, Utc};
    use vector_core::event::{LogEvent, Metric, MetricKind, MetricValue, StatisticKind, Value};
    use vector_core::metric_tags;
    use vrl::btreemap;

    use super::*;

    #[test]
    fn serialize_json_log() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "x" => Value::from("23"),
            "z" => Value::from(25),
            "a" => Value::from("0"),
        }));
        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(bytes, r#"{"a":"0","x":"23","z":25}"#);
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
            .with_tags(Some(metric_tags!(
                "key2" => "value2",
                "key1" => "value1",
                "Key3" => "Value3",
            )))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
                    .single()
                    .and_then(|t| t.with_nanosecond(11))
                    .expect("invalid timestamp"),
            )),
        );

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
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

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
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

        let bytes = serialize(JsonSerializerConfig::default(), event);

        assert_eq!(
            bytes,
            r#"{"name":"glork","kind":"incremental","distribution":{"samples":[{"value":10.0,"rate":1}],"statistic":"histogram"}}"#
        );
    }

    #[test]
    fn serialize_equals_to_json_value() {
        let event = Event::Log(LogEvent::from(btreemap! {
            "foo" => Value::from("bar")
        }));
        let mut serializer = JsonSerializerConfig::default().build();
        let mut bytes = BytesMut::new();

        serializer.encode(event.clone(), &mut bytes).unwrap();

        let json = serializer.to_json_value(event).unwrap();

        assert_eq!(bytes.freeze(), serde_json::to_string(&json).unwrap());
    }

    #[test]
    fn serialize_metric_tags_full() {
        let bytes = serialize(
            JsonSerializerConfig {
                metric_tag_values: MetricTagValues::Full,
            },
            metric2(),
        );

        assert_eq!(
            bytes,
            r#"{"name":"counter","tags":{"a":["first",null,"second"]},"kind":"incremental","counter":{"value":1.0}}"#
        );
    }

    #[test]
    fn serialize_metric_tags_single() {
        let bytes = serialize(
            JsonSerializerConfig {
                metric_tag_values: MetricTagValues::Single,
            },
            metric2(),
        );

        assert_eq!(
            bytes,
            r#"{"name":"counter","tags":{"a":"second"},"kind":"incremental","counter":{"value":1.0}}"#
        );
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

    fn serialize(config: JsonSerializerConfig, input: Event) -> Bytes {
        let mut buffer = BytesMut::new();
        config.build().encode(input, &mut buffer).unwrap();
        buffer.freeze()
    }
}

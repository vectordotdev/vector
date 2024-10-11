use std::borrow::Cow;

use bytes::Bytes;
use chrono::DateTime;
use derivative::Derivative;
use influxdb_line_protocol::{FieldValue, ParsedLine};
use smallvec::SmallVec;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;
use vector_core::event::{Event, Metric, MetricKind, MetricTags, MetricValue};
use vector_core::{config::DataType, schema};
use vrl::value::kind::Collection;
use vrl::value::Kind;

use crate::decoding::format::default_lossy;

use super::Deserializer;

/// Config used to build a `InfluxdbDeserializer`.
/// - [InfluxDB Line Protocol](https://docs.influxdata.com/influxdb/v1/write_protocols/line_protocol_tutorial/):
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct InfluxdbDeserializerConfig {
    /// Influxdb-specific decoding options.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub influxdb: InfluxdbDeserializerOptions,
}

impl InfluxdbDeserializerConfig {
    /// new constructs a new InfluxdbDeserializerConfig
    pub fn new(options: InfluxdbDeserializerOptions) -> Self {
        Self { influxdb: options }
    }

    /// build constructs a new InfluxdbDeserializer
    pub fn build(&self) -> InfluxdbDeserializer {
        Into::<InfluxdbDeserializer>::into(self)
    }

    /// The output type produced by the deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Metric
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        schema::Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [log_namespace],
        )
    }
}

/// Influxdb-specific decoding options.
#[configurable_component]
#[derive(Debug, Clone, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub struct InfluxdbDeserializerOptions {
    /// Determines whether or not to replace invalid UTF-8 sequences instead of failing.
    ///
    /// When true, invalid UTF-8 sequences are replaced with the [`U+FFFD REPLACEMENT CHARACTER`][U+FFFD].
    ///
    /// [U+FFFD]: https://en.wikipedia.org/wiki/Specials_(Unicode_block)#Replacement_character
    #[serde(
        default = "default_lossy",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    #[derivative(Default(value = "default_lossy()"))]
    pub lossy: bool,
}

/// Deserializer for the influxdb line protocol
#[derive(Debug, Clone, Derivative)]
#[derivative(Default)]
pub struct InfluxdbDeserializer {
    #[derivative(Default(value = "default_lossy()"))]
    lossy: bool,
}

impl InfluxdbDeserializer {
    /// new constructs a new InfluxdbDeserializer
    pub fn new(lossy: bool) -> Self {
        Self { lossy }
    }
}

impl Deserializer for InfluxdbDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        _log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let line: Cow<str> = match self.lossy {
            true => String::from_utf8_lossy(&bytes),
            false => Cow::from(std::str::from_utf8(&bytes)?),
        };
        let parsed_line = influxdb_line_protocol::parse_lines(&line);

        let res = parsed_line
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .flat_map(|line| {
                let ParsedLine {
                    series,
                    field_set,
                    timestamp,
                } = line;

                field_set
                    .iter()
                    .filter_map(|f| {
                        let measurement = series.measurement.clone();
                        let tags = series.tag_set.as_ref();
                        let val = match f.1 {
                            FieldValue::I64(v) => v as f64,
                            FieldValue::U64(v) => v as f64,
                            FieldValue::F64(v) => v,
                            FieldValue::Boolean(v) => {
                                if v {
                                    1.0
                                } else {
                                    0.0
                                }
                            }
                            FieldValue::String(_) => return None, // String values cannot be modelled in our schema
                        };
                        Some(Event::Metric(
                            Metric::new(
                                format!("{0}_{1}", measurement, f.0),
                                MetricKind::Absolute,
                                MetricValue::Gauge { value: val },
                            )
                            .with_tags(tags.map(|ts| {
                                MetricTags::from_iter(
                                    ts.iter().map(|t| (t.0.to_string(), t.1.to_string())),
                                )
                            }))
                            .with_timestamp(timestamp.map(DateTime::from_timestamp_nanos)),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        Ok(res)
    }
}

impl From<&InfluxdbDeserializerConfig> for InfluxdbDeserializer {
    fn from(config: &InfluxdbDeserializerConfig) -> Self {
        Self {
            lossy: config.influxdb.lossy,
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use vector_core::{
        config::LogNamespace,
        event::{Metric, MetricKind, MetricTags, MetricValue},
    };

    use crate::decoding::format::{Deserializer, InfluxdbDeserializer};

    #[test]
    fn deserialize_success() {
        let deser = InfluxdbDeserializer::new(true);
        let now = chrono::Utc::now();
        let now_timestamp_nanos = now.timestamp_nanos_opt().unwrap();
        let buffer = Bytes::from(format!(
            "cpu,host=A,region=west usage_system=64i,usage_user=10i {now_timestamp_nanos}"
        ));
        let events = deser.parse(buffer, LogNamespace::default()).unwrap();
        assert_eq!(events.len(), 2);

        assert_eq!(
            events[0].as_metric(),
            &Metric::new(
                "cpu_usage_system",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 64. },
            )
            .with_tags(Some(MetricTags::from_iter([
                ("host".to_string(), "A".to_string()),
                ("region".to_string(), "west".to_string()),
            ])))
            .with_timestamp(Some(now))
        );
        assert_eq!(
            events[1].as_metric(),
            &Metric::new(
                "cpu_usage_user",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 10. },
            )
            .with_tags(Some(MetricTags::from_iter([
                ("host".to_string(), "A".to_string()),
                ("region".to_string(), "west".to_string()),
            ])))
            .with_timestamp(Some(now))
        );
    }

    #[test]
    fn deserialize_error() {
        let deser = InfluxdbDeserializer::new(true);
        let buffer = Bytes::from("some invalid string");
        assert!(deser.parse(buffer, LogNamespace::default()).is_err());
    }
}

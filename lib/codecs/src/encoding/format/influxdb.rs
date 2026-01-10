use std::{collections::HashMap, sync::Arc};

use bytes::{BufMut, BytesMut};
use chrono::{DateTime, Utc};
use derivative::Derivative;
use snafu::Snafu;
use vector_config::configurable_component;
use vector_core::{
    config::DataType,
    event::{
        Event, KeyString, MetricTags,
        metric::{Metric, MetricSketch, MetricValue, Quantile, Sample, StatisticKind},
    },
    schema,
};

/// Representation of a field value in the Influx line protocol.
#[derive(Clone, Debug)]
pub enum Field {
    /// String field.
    String(String),
    /// Floating point field.
    Float(f64),
    /// Unsigned integer field.
    UnsignedInt(u64),
    /// Signed integer field.
    Int(i64),
    /// Boolean field.
    Bool(bool),
}

/// Influx line protocol version.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolVersion {
    /// Line protocol for InfluxDB v1.x.
    V1,
    /// Line protocol for InfluxDB v2.x.
    #[derivative(Default)]
    V2,
}

/// Serializer configuration for encoding metrics as Influx line protocol.
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct InfluxLineProtocolSerializerConfig {
    /// The protocol version to encode fields with.
    #[serde(default)]
    pub protocol_version: ProtocolVersion,

    /// A default namespace applied when metrics do not specify one.
    #[serde(default)]
    pub default_namespace: Option<String>,

    /// Additional tags that are appended to every encoded metric.
    #[configurable(metadata(
        docs::additional_props_description = "A tag key/value pair that is appended to each measurement."
    ))]
    #[serde(default)]
    pub tags: Option<HashMap<String, String>>,

    /// Quantiles to calculate when encoding distribution metrics.
    #[serde(default = "default_summary_quantiles")]
    pub quantiles: Vec<f64>,
}

impl InfluxLineProtocolSerializerConfig {
    /// Build the serializer from this configuration.
    pub fn build(&self) -> InfluxLineProtocolSerializer {
        InfluxLineProtocolSerializer::new(self.clone())
    }

    /// The data type of events accepted by this serializer.
    pub fn input_type(&self) -> DataType {
        DataType::Metric
    }

    /// The schema requirement for events encoded by this serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Default quantiles that align with the existing InfluxDB sink behaviour.
pub fn default_summary_quantiles() -> Vec<f64> {
    vec![0.5, 0.75, 0.9, 0.95, 0.99]
}

/// Serializer that emits metrics encoded with the Influx line protocol.
#[derive(Clone, Debug)]
pub struct InfluxLineProtocolSerializer {
    protocol_version: ProtocolVersion,
    default_namespace: Option<String>,
    tags: Option<Arc<HashMap<String, String>>>,
    quantiles: Arc<Vec<f64>>,
}

impl InfluxLineProtocolSerializer {
    /// Creates a new serializer from the provided configuration.
    pub fn new(config: InfluxLineProtocolSerializerConfig) -> Self {
        Self {
            protocol_version: config.protocol_version,
            default_namespace: config.default_namespace,
            tags: config.tags.map(Arc::new),
            quantiles: Arc::new(config.quantiles),
        }
    }

    fn encode_metric(
        &self,
        metric: Metric,
        output: &mut BytesMut,
    ) -> Result<(), InfluxLineProtocolSerializerError> {
        let namespace = metric.namespace().or(self.default_namespace.as_deref());
        let measurement = encode_namespace(namespace, '.', metric.name());
        let timestamp = encode_timestamp(metric.timestamp());
        let tags = merge_tags(&metric, self.tags.as_deref());
        let (metric_type, fields) = get_type_and_fields(metric.value(), &self.quantiles);

        let mut merged_tags = tags.unwrap_or_default();
        merged_tags.replace("metric_type".to_owned(), metric_type.to_owned());

        influx_line_protocol(
            self.protocol_version,
            &measurement,
            Some(merged_tags),
            fields,
            timestamp,
            output,
        )
        .map_err(|message| InfluxLineProtocolSerializerError::LineProtocol { message })
    }
}

impl tokio_util::codec::Encoder<Event> for InfluxLineProtocolSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, item: Event, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            Event::Metric(metric) => self.encode_metric(metric, dst).map_err(Into::into),
            Event::Log(_) => {
                Err(InfluxLineProtocolSerializerError::InvalidEventType { actual: "log" }.into())
            }
            Event::Trace(_) => {
                Err(InfluxLineProtocolSerializerError::InvalidEventType { actual: "trace" }.into())
            }
        }
    }
}

/// Errors returned by the Influx line protocol serializer.
#[derive(Debug, Snafu)]
pub enum InfluxLineProtocolSerializerError {
    /// The encoder received an event that was not a metric.
    #[snafu(display("Influx line protocol encoding expects metric events, got {actual}"))]
    InvalidEventType {
        /// The kind of non-metric event encountered.
        actual: &'static str,
    },
    /// The encoder could not construct a valid line protocol frame.
    #[snafu(display("Influx line protocol error: {message}"))]
    LineProtocol {
        /// The message describing why line protocol generation failed.
        message: &'static str,
    },
}

fn merge_tags(metric: &Metric, extra: Option<&HashMap<String, String>>) -> Option<MetricTags> {
    match (metric.tags().cloned(), extra) {
        (Some(mut metric_tags), Some(extra)) => {
            metric_tags.extend(
                extra
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            Some(metric_tags)
        }
        (Some(metric_tags), None) => Some(metric_tags),
        (None, Some(extra)) => Some(
            extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        (None, None) => None,
    }
}

fn get_type_and_fields(
    value: &MetricValue,
    quantiles: &[f64],
) -> (&'static str, Option<HashMap<KeyString, Field>>) {
    match value {
        MetricValue::Counter { value } => ("counter", Some(to_fields(*value))),
        MetricValue::Gauge { value } => ("gauge", Some(to_fields(*value))),
        MetricValue::Set { values } => ("set", Some(to_fields(values.len() as f64))),
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            let mut fields: HashMap<KeyString, Field> = buckets
                .iter()
                .map(|sample| {
                    (
                        format!("bucket_{}", sample.upper_limit).into(),
                        Field::UnsignedInt(sample.count),
                    )
                })
                .collect();
            fields.insert("count".into(), Field::UnsignedInt(*count));
            fields.insert("sum".into(), Field::Float(*sum));

            ("histogram", Some(fields))
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            let mut fields: HashMap<KeyString, Field> = quantiles
                .iter()
                .map(|quantile| {
                    (
                        format!("quantile_{}", quantile.quantile).into(),
                        Field::Float(quantile.value),
                    )
                })
                .collect();
            fields.insert("count".into(), Field::UnsignedInt(*count));
            fields.insert("sum".into(), Field::Float(*sum));

            ("summary", Some(fields))
        }
        MetricValue::Distribution { samples, statistic } => {
            let quantiles = match statistic {
                StatisticKind::Histogram => &[0.95] as &[f64],
                StatisticKind::Summary => quantiles,
            };
            let fields = encode_distribution(samples, quantiles);
            ("distribution", fields)
        }
        MetricValue::Sketch { sketch } => match sketch {
            MetricSketch::AgentDDSketch(ddsketch) => {
                let mut fields = [0.5, 0.75, 0.9, 0.99]
                    .iter()
                    .map(|q| {
                        let quantile = Quantile {
                            quantile: *q,
                            value: ddsketch.quantile(*q).unwrap_or(0.0),
                        };
                        (
                            quantile.to_percentile_string().into(),
                            Field::Float(quantile.value),
                        )
                    })
                    .collect::<HashMap<KeyString, _>>();
                fields.insert(
                    "count".into(),
                    Field::UnsignedInt(u64::from(ddsketch.count())),
                );
                fields.insert(
                    "min".into(),
                    Field::Float(ddsketch.min().unwrap_or(f64::MAX)),
                );
                fields.insert(
                    "max".into(),
                    Field::Float(ddsketch.max().unwrap_or(f64::MIN)),
                );
                fields.insert("sum".into(), Field::Float(ddsketch.sum().unwrap_or(0.0)));
                fields.insert("avg".into(), Field::Float(ddsketch.avg().unwrap_or(0.0)));

                ("sketch", Some(fields))
            }
        },
    }
}

fn encode_distribution(samples: &[Sample], quantiles: &[f64]) -> Option<HashMap<KeyString, Field>> {
    let statistic = DistributionStatistic::from_samples(samples, quantiles)?;

    Some(
        [
            ("min".into(), Field::Float(statistic.min)),
            ("max".into(), Field::Float(statistic.max)),
            ("median".into(), Field::Float(statistic.median)),
            ("avg".into(), Field::Float(statistic.avg)),
            ("sum".into(), Field::Float(statistic.sum)),
            ("count".into(), Field::Float(statistic.count as f64)),
        ]
        .into_iter()
        .chain(
            statistic
                .quantiles
                .iter()
                .map(|&(p, val)| (format!("quantile_{p:.2}").into(), Field::Float(val))),
        )
        .collect(),
    )
}

fn to_fields(value: f64) -> HashMap<KeyString, Field> {
    [("value".into(), Field::Float(value))]
        .into_iter()
        .collect()
}

fn encode_namespace(namespace: Option<&str>, delimiter: char, name: &str) -> String {
    namespace
        .map(|namespace| format!("{namespace}{delimiter}{name}"))
        .unwrap_or_else(|| name.to_owned())
}

/// Encode a full InfluxDB line protocol entry into the provided buffer.
pub fn influx_line_protocol(
    protocol_version: ProtocolVersion,
    measurement: &str,
    tags: Option<MetricTags>,
    fields: Option<HashMap<KeyString, Field>>,
    timestamp: i64,
    buffer: &mut BytesMut,
) -> Result<(), &'static str> {
    let fields = fields.unwrap_or_default();
    if fields.is_empty() {
        return Err("fields must not be empty");
    }

    encode_string(measurement, buffer);

    if let Some(tags) = tags {
        let mut tags_buffer = BytesMut::new();
        encode_tags(tags, &mut tags_buffer);
        if !tags_buffer.is_empty() {
            buffer.put_u8(b',');
            buffer.extend_from_slice(&tags_buffer);
        }
    }

    buffer.put_u8(b' ');
    encode_fields(protocol_version, fields, buffer);
    buffer.put_u8(b' ');
    buffer.put_slice(&timestamp.to_string().into_bytes());
    buffer.put_u8(b'\n');
    Ok(())
}

fn encode_tags(tags: MetricTags, output: &mut BytesMut) {
    let original_len = output.len();
    for (key, value) in tags.iter_single() {
        if key.is_empty() || value.is_empty() {
            continue;
        }
        encode_string(key, output);
        output.put_u8(b'=');
        encode_string(value, output);
        output.put_u8(b',');
    }

    if output.len() > original_len {
        output.truncate(output.len() - 1);
    }
}

fn encode_fields(
    protocol_version: ProtocolVersion,
    fields: HashMap<KeyString, Field>,
    output: &mut BytesMut,
) {
    let original_len = output.len();

    for (key, value) in fields.into_iter() {
        encode_string(&key, output);
        output.put_u8(b'=');

        match value {
            Field::String(s) => {
                output.put_u8(b'"');
                for ch in s.chars() {
                    if "\\\"".contains(ch) {
                        output.put_u8(b'\\');
                    }
                    let mut buffer: [u8; 4] = [0; 4];
                    output.put_slice(ch.encode_utf8(&mut buffer).as_bytes());
                }
                output.put_u8(b'"');
            }
            Field::Float(f) => output.put_slice(&f.to_string().into_bytes()),
            Field::UnsignedInt(i) => {
                output.put_slice(&i.to_string().into_bytes());
                let suffix = match protocol_version {
                    ProtocolVersion::V1 => 'i',
                    ProtocolVersion::V2 => 'u',
                };
                let mut buffer: [u8; 4] = [0; 4];
                output.put_slice(suffix.encode_utf8(&mut buffer).as_bytes());
            }
            Field::Int(i) => {
                output.put_slice(&i.to_string().into_bytes());
                output.put_u8(b'i');
            }
            Field::Bool(b) => {
                output.put_slice(&b.to_string().into_bytes());
            }
        }

        output.put_u8(b',');
    }

    if output.len() > original_len {
        output.truncate(output.len() - 1);
    }
}

fn encode_string(value: &str, output: &mut BytesMut) {
    for ch in value.chars() {
        if "\\, =".contains(ch) {
            output.put_u8(b'\\');
        }
        let mut buffer: [u8; 4] = [0; 4];
        output.put_slice(ch.encode_utf8(&mut buffer).as_bytes());
    }
}

/// Converts an optional timestamp into the nanoseconds expected by the protocol.
pub fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    match timestamp {
        Some(timestamp) => timestamp
            .timestamp_nanos_opt()
            .expect("timestamp out of range for nanosecond precision"),
        None => encode_timestamp(Some(Utc::now())),
    }
}

#[derive(Debug, Clone)]
struct DistributionStatistic {
    min: f64,
    max: f64,
    median: f64,
    avg: f64,
    sum: f64,
    count: u64,
    quantiles: Vec<(f64, f64)>,
}

impl DistributionStatistic {
    fn from_samples(source: &[Sample], quantiles: &[f64]) -> Option<Self> {
        let mut bins = source
            .iter()
            .filter(|sample| sample.rate > 0)
            .copied()
            .collect::<Vec<_>>();

        match bins.len() {
            0 => None,
            1 => {
                let val = bins[0].value;
                let count = bins[0].rate;
                Some(Self {
                    min: val,
                    max: val,
                    median: val,
                    avg: val,
                    sum: val * count as f64,
                    count: count as u64,
                    quantiles: quantiles.iter().map(|&p| (p, val)).collect(),
                })
            }
            _ => {
                bins.sort_unstable_by(|a, b| a.value.total_cmp(&b.value));

                // SAFETY: bins.len() >= 2 in this match arm
                let min = bins.first().expect("bins is non-empty").value;
                let max = bins.last().expect("bins is non-empty").value;
                let sum = bins
                    .iter()
                    .map(|sample| sample.value * sample.rate as f64)
                    .sum::<f64>();

                for index in 1..bins.len() {
                    bins[index].rate += bins[index - 1].rate;
                }

                let count = bins.last().expect("bins is non-empty").rate;
                let avg = sum / count as f64;

                let median = find_quantile(&bins, 0.5);
                let quantiles = quantiles
                    .iter()
                    .map(|&p| (p, find_quantile(&bins, p)))
                    .collect();

                Some(Self {
                    min,
                    max,
                    median,
                    avg,
                    sum,
                    count: count as u64,
                    quantiles,
                })
            }
        }
    }
}

fn find_quantile(bins: &[Sample], p: f64) -> f64 {
    let count = bins.last().expect("bins is empty").rate;
    find_sample(bins, (p * count as f64).round() as u32)
}

fn find_sample(bins: &[Sample], i: u32) -> f64 {
    let index = match bins.binary_search_by_key(&i, |sample| sample.rate) {
        Ok(index) => index,
        Err(index) => index,
    };
    bins[index].value
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use tokio_util::codec::Encoder;
    use vector_core::{
        config::DataType,
        event::{
            MetricTags,
            metric::{Metric, MetricKind, MetricValue},
        },
    };

    #[test]
    fn encode_counter_metric() {
        let mut serializer =
            InfluxLineProtocolSerializer::new(InfluxLineProtocolSerializerConfig {
                default_namespace: Some("ns".to_string()),
                ..Default::default()
            });
        let metric = Metric::new(
            "total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.5 },
        )
        .with_timestamp(Some(Utc::now()));

        let mut buffer = BytesMut::new();
        serializer
            .encode(Event::Metric(metric), &mut buffer)
            .unwrap();
        assert!(
            String::from_utf8(buffer.to_vec())
                .unwrap()
                .starts_with("ns.total,metric_type=counter value=1.5 ")
        );
    }

    #[test]
    fn encode_tags_are_escaped() {
        let measurement = "cpu";
        let mut buffer = BytesMut::new();
        let mut tags = MetricTags::default();
        tags.insert("host name".to_owned(), "a=b".to_owned());
        influx_line_protocol(
            ProtocolVersion::V2,
            measurement,
            Some(tags),
            Some(HashMap::from([("v".into(), Field::Float(1.0))])),
            42,
            &mut buffer,
        )
        .unwrap();

        assert_eq!(
            String::from_utf8(buffer.to_vec()).unwrap(),
            "cpu,host\\ name=a\\=b v=1 42\n"
        );
    }

    #[test]
    fn serializer_schema() {
        let config = InfluxLineProtocolSerializerConfig::default();
        assert_eq!(config.input_type(), DataType::Metric);
    }
}

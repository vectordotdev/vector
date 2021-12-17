use chrono::TimeZone;

use crate::{
    event::{self, BTreeMap, WithMetadata},
    metrics::AgentDDSketch,
};

include!(concat!(env!("OUT_DIR"), "/event.rs"));
pub use event_wrapper::Event;
pub use metric::Value as MetricValue;

use super::metric::MetricSketch;

impl From<Event> for EventWrapper {
    fn from(event: Event) -> Self {
        Self { event: Some(event) }
    }
}

impl From<Log> for Event {
    fn from(log: Log) -> Self {
        Self::Log(log)
    }
}

impl From<Metric> for Event {
    fn from(metric: Metric) -> Self {
        Self::Metric(metric)
    }
}

impl From<Log> for event::LogEvent {
    fn from(log: Log) -> Self {
        let fields = log
            .fields
            .into_iter()
            .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
            .collect::<BTreeMap<_, _>>();

        Self::from(fields)
    }
}

impl From<Metric> for event::Metric {
    fn from(metric: Metric) -> Self {
        let kind = match metric.kind() {
            metric::Kind::Incremental => event::MetricKind::Incremental,
            metric::Kind::Absolute => event::MetricKind::Absolute,
        };

        let name = metric.name;

        let namespace = if metric.namespace.is_empty() {
            None
        } else {
            Some(metric.namespace)
        };

        let timestamp = metric
            .timestamp
            .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

        let tags = if metric.tags.is_empty() {
            None
        } else {
            Some(metric.tags)
        };

        let value = match metric.value.unwrap() {
            MetricValue::Counter(counter) => event::MetricValue::Counter {
                value: counter.value,
            },
            MetricValue::Gauge(gauge) => event::MetricValue::Gauge { value: gauge.value },
            MetricValue::Set(set) => event::MetricValue::Set {
                values: set.values.into_iter().collect(),
            },
            MetricValue::Distribution1(dist) => event::MetricValue::Distribution {
                statistic: dist.statistic().into(),
                samples: event::metric::zip_samples(dist.values, dist.sample_rates),
            },
            MetricValue::Distribution2(dist) => event::MetricValue::Distribution {
                statistic: dist.statistic().into(),
                samples: dist.samples.into_iter().map(Into::into).collect(),
            },
            MetricValue::AggregatedHistogram1(hist) => event::MetricValue::AggregatedHistogram {
                buckets: event::metric::zip_buckets(hist.buckets, hist.counts),
                count: hist.count,
                sum: hist.sum,
            },
            MetricValue::AggregatedHistogram2(hist) => event::MetricValue::AggregatedHistogram {
                buckets: hist.buckets.into_iter().map(Into::into).collect(),
                count: hist.count,
                sum: hist.sum,
            },
            MetricValue::AggregatedSummary1(summary) => event::MetricValue::AggregatedSummary {
                quantiles: event::metric::zip_quantiles(summary.quantiles, summary.values),
                count: summary.count,
                sum: summary.sum,
            },
            MetricValue::AggregatedSummary2(summary) => event::MetricValue::AggregatedSummary {
                quantiles: summary.quantiles.into_iter().map(Into::into).collect(),
                count: summary.count,
                sum: summary.sum,
            },
            MetricValue::Sketch(sketch) => match sketch.sketch.unwrap() {
                sketch::Sketch::AgentDdSketch(ddsketch) => event::MetricValue::Sketch {
                    sketch: ddsketch.into(),
                },
            },
        };

        Self::new(name, kind, value)
            .with_namespace(namespace)
            .with_tags(tags)
            .with_timestamp(timestamp)
    }
}

impl From<EventWrapper> for event::Event {
    fn from(proto: EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => Self::Log(proto.into()),
            Event::Metric(proto) => Self::Metric(proto.into()),
        }
    }
}

impl From<event::LogEvent> for Log {
    fn from(log_event: event::LogEvent) -> Self {
        WithMetadata::<Self>::from(log_event).data
    }
}

impl From<event::LogEvent> for WithMetadata<Log> {
    fn from(log_event: event::LogEvent) -> Self {
        let (fields, metadata) = log_event.into_parts();
        let fields = fields
            .into_iter()
            .map(|(k, v)| (k, encode_value(v)))
            .collect::<BTreeMap<_, _>>();

        let data = Log { fields };
        Self { data, metadata }
    }
}

impl From<event::Metric> for Metric {
    fn from(metric: event::Metric) -> Self {
        WithMetadata::<Self>::from(metric).data
    }
}

impl From<event::Metric> for WithMetadata<Metric> {
    fn from(metric: event::Metric) -> Self {
        let (series, data, metadata) = metric.into_parts();
        let name = series.name.name;
        let namespace = series.name.namespace.unwrap_or_default();

        let timestamp = data.timestamp.map(|ts| prost_types::Timestamp {
            seconds: ts.timestamp(),
            nanos: ts.timestamp_subsec_nanos() as i32,
        });

        let tags = series.tags.unwrap_or_default();

        let kind = match data.kind {
            event::MetricKind::Incremental => metric::Kind::Incremental,
            event::MetricKind::Absolute => metric::Kind::Absolute,
        }
        .into();

        let metric = match data.value {
            event::MetricValue::Counter { value } => MetricValue::Counter(Counter { value }),
            event::MetricValue::Gauge { value } => MetricValue::Gauge(Gauge { value }),
            event::MetricValue::Set { values } => MetricValue::Set(Set {
                values: values.into_iter().collect(),
            }),
            event::MetricValue::Distribution { samples, statistic } => {
                MetricValue::Distribution2(Distribution2 {
                    samples: samples.into_iter().map(Into::into).collect(),
                    statistic: match statistic {
                        event::StatisticKind::Histogram => StatisticKind::Histogram,
                        event::StatisticKind::Summary => StatisticKind::Summary,
                    }
                    .into(),
                })
            }
            event::MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => MetricValue::AggregatedHistogram2(AggregatedHistogram2 {
                buckets: buckets.into_iter().map(Into::into).collect(),
                count,
                sum,
            }),
            event::MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => MetricValue::AggregatedSummary2(AggregatedSummary2 {
                quantiles: quantiles.into_iter().map(Into::into).collect(),
                count,
                sum,
            }),
            event::MetricValue::Sketch { sketch } => match sketch {
                MetricSketch::AgentDDSketch(ddsketch) => {
                    let bin_map = ddsketch.bin_map();
                    let (keys, counts) = bin_map.into_parts();
                    let keys = keys.into_iter().map(i32::from).collect();
                    let counts = counts.into_iter().map(u32::from).collect();

                    MetricValue::Sketch(Sketch {
                        sketch: Some(sketch::Sketch::AgentDdSketch(sketch::AgentDdSketch {
                            count: ddsketch.count(),
                            min: ddsketch.min().unwrap_or(f64::MAX),
                            max: ddsketch.max().unwrap_or(f64::MIN),
                            sum: ddsketch.sum().unwrap_or(0.0),
                            avg: ddsketch.avg().unwrap_or(0.0),
                            k: keys,
                            n: counts,
                        })),
                    })
                }
            },
        };

        let data = Metric {
            name,
            namespace,
            timestamp,
            tags,
            kind,
            value: Some(metric),
        };
        Self { data, metadata }
    }
}

impl From<event::Event> for Event {
    fn from(event: event::Event) -> Self {
        WithMetadata::<Self>::from(event).data
    }
}

impl From<event::Event> for WithMetadata<Event> {
    fn from(event: event::Event) -> Self {
        match event {
            event::Event::Log(log_event) => WithMetadata::<Log>::from(log_event).into(),
            event::Event::Metric(metric) => WithMetadata::<Metric>::from(metric).into(),
        }
    }
}

impl From<event::Event> for EventWrapper {
    fn from(event: event::Event) -> Self {
        WithMetadata::<EventWrapper>::from(event).data
    }
}

impl From<event::Event> for WithMetadata<EventWrapper> {
    fn from(event: event::Event) -> Self {
        WithMetadata::<Event>::from(event).into()
    }
}

impl From<AgentDDSketch> for Sketch {
    fn from(ddsketch: AgentDDSketch) -> Self {
        let bin_map = ddsketch.bin_map();
        let (keys, counts) = bin_map.into_parts();
        let ddsketch = sketch::AgentDdSketch {
            count: ddsketch.count(),
            min: ddsketch.min().unwrap_or(f64::MAX),
            max: ddsketch.max().unwrap_or(f64::MIN),
            sum: ddsketch.sum().unwrap_or(0.0),
            avg: ddsketch.avg().unwrap_or(0.0),
            k: keys.into_iter().map(i32::from).collect(),
            n: counts.into_iter().map(u32::from).collect(),
        };
        Sketch {
            sketch: Some(sketch::Sketch::AgentDdSketch(ddsketch)),
        }
    }
}

impl From<sketch::AgentDdSketch> for MetricSketch {
    fn from(sketch: sketch::AgentDdSketch) -> Self {
        // These safe conversions are annoying because the Datadog Agent internally uses i16/u16,
        // but the proto definition uses i32/u32, so we have to jump through these hoops.
        let keys = sketch
            .k
            .into_iter()
            .map(|k| (k, k > 0))
            .map(|(k, pos)| {
                k.try_into()
                    .unwrap_or_else(|_| if pos { i16::MAX } else { i16::MIN })
            })
            .collect::<Vec<_>>();
        let counts = sketch
            .n
            .into_iter()
            .map(|n| n.try_into().unwrap_or(u16::MAX))
            .collect::<Vec<_>>();
        MetricSketch::AgentDDSketch(
            AgentDDSketch::from_raw(
                sketch.count as u32,
                sketch.min,
                sketch.max,
                sketch.sum,
                sketch.avg,
                &keys,
                &counts,
            )
            .expect("keys/counts were unexpectedly mismatched"),
        )
    }
}

fn decode_value(input: Value) -> Option<event::Value> {
    match input.kind {
        Some(value::Kind::RawBytes(data)) => Some(event::Value::Bytes(data)),
        Some(value::Kind::Timestamp(ts)) => Some(event::Value::Timestamp(
            chrono::Utc.timestamp(ts.seconds, ts.nanos as u32),
        )),
        Some(value::Kind::Integer(value)) => Some(event::Value::Integer(value)),
        Some(value::Kind::Float(value)) => Some(event::Value::Float(value)),
        Some(value::Kind::Boolean(value)) => Some(event::Value::Boolean(value)),
        Some(value::Kind::Map(map)) => decode_map(map.fields),
        Some(value::Kind::Array(array)) => decode_array(array.items),
        Some(value::Kind::Null(_)) => Some(event::Value::Null),
        None => {
            error!("Encoded event contains unknown value kind.");
            None
        }
    }
}

fn decode_map(fields: BTreeMap<String, Value>) -> Option<event::Value> {
    let mut accum: BTreeMap<String, event::Value> = BTreeMap::new();
    for (key, value) in fields {
        match decode_value(value) {
            Some(value) => {
                accum.insert(key, value);
            }
            None => return None,
        }
    }
    Some(event::Value::Map(accum))
}

fn decode_array(items: Vec<Value>) -> Option<event::Value> {
    let mut accum = Vec::with_capacity(items.len());
    for value in items {
        match decode_value(value) {
            Some(value) => accum.push(value),
            None => return None,
        }
    }
    Some(event::Value::Array(accum))
}

fn encode_value(value: event::Value) -> Value {
    Value {
        kind: match value {
            event::Value::Bytes(b) => Some(value::Kind::RawBytes(b)),
            event::Value::Timestamp(ts) => Some(value::Kind::Timestamp(prost_types::Timestamp {
                seconds: ts.timestamp(),
                nanos: ts.timestamp_subsec_nanos() as i32,
            })),
            event::Value::Integer(value) => Some(value::Kind::Integer(value)),
            event::Value::Float(value) => Some(value::Kind::Float(value)),
            event::Value::Boolean(value) => Some(value::Kind::Boolean(value)),
            event::Value::Map(fields) => Some(value::Kind::Map(encode_map(fields))),
            event::Value::Array(items) => Some(value::Kind::Array(encode_array(items))),
            event::Value::Null => Some(value::Kind::Null(ValueNull::NullValue as i32)),
        },
    }
}

fn encode_map(fields: BTreeMap<String, event::Value>) -> ValueMap {
    ValueMap {
        fields: fields
            .into_iter()
            .map(|(key, value)| (key, encode_value(value)))
            .collect(),
    }
}

fn encode_array(items: Vec<event::Value>) -> ValueArray {
    ValueArray {
        items: items.into_iter().map(encode_value).collect(),
    }
}

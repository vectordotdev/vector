use chrono::TimeZone;
use ordered_float::NotNan;

use crate::{
    event::{self, BTreeMap, MetricTags, WithMetadata},
    metrics::AgentDDSketch,
};

#[allow(warnings, clippy::all, clippy::pedantic)]
mod proto_event {
    include!(concat!(env!("OUT_DIR"), "/event.rs"));
}
pub use event_wrapper::Event;
pub use metric::Value as MetricValue;
pub use proto_event::*;
use vrl::value::Value as VrlValue;

use super::{array, metric::MetricSketch};

impl event_array::Events {
    // We can't use the standard `From` traits here because the actual
    // type of `LogArray` and `TraceArray` are the same.
    fn from_logs(logs: array::LogArray) -> Self {
        let logs = logs.into_iter().map(Into::into).collect();
        Self::Logs(LogArray { logs })
    }

    fn from_metrics(metrics: array::MetricArray) -> Self {
        let metrics = metrics.into_iter().map(Into::into).collect();
        Self::Metrics(MetricArray { metrics })
    }

    fn from_traces(traces: array::TraceArray) -> Self {
        let traces = traces.into_iter().map(Into::into).collect();
        Self::Traces(TraceArray { traces })
    }
}

impl From<array::EventArray> for EventArray {
    fn from(events: array::EventArray) -> Self {
        let events = Some(match events {
            array::EventArray::Logs(array) => event_array::Events::from_logs(array),
            array::EventArray::Metrics(array) => event_array::Events::from_metrics(array),
            array::EventArray::Traces(array) => event_array::Events::from_traces(array),
        });
        Self { events }
    }
}

impl From<EventArray> for array::EventArray {
    fn from(events: EventArray) -> Self {
        let events = events.events.unwrap();

        match events {
            event_array::Events::Logs(logs) => {
                array::EventArray::Logs(logs.logs.into_iter().map(Into::into).collect())
            }
            event_array::Events::Metrics(metrics) => {
                array::EventArray::Metrics(metrics.metrics.into_iter().map(Into::into).collect())
            }
            event_array::Events::Traces(traces) => {
                array::EventArray::Traces(traces.traces.into_iter().map(Into::into).collect())
            }
        }
    }
}

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

impl From<Trace> for Event {
    fn from(trace: Trace) -> Self {
        Self::Trace(trace)
    }
}

impl From<Log> for event::LogEvent {
    fn from(log: Log) -> Self {
        let mut event_log = if let Some(value) = log.value {
            Self::from(decode_value(value).unwrap_or(VrlValue::Null))
        } else {
            // This is for backwards compatibility. Only `value` should be set
            let fields = log
                .fields
                .into_iter()
                .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
                .collect::<BTreeMap<_, _>>();

            Self::from(fields)
        };

        if let Some(metadata_value) = log.metadata {
            if let Some(decoded_value) = decode_value(metadata_value) {
                *event_log.metadata_mut().value_mut() = decoded_value;
            }
        }
        event_log
    }
}

impl From<Trace> for event::TraceEvent {
    fn from(trace: Trace) -> Self {
        let fields = trace
            .fields
            .into_iter()
            .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
            .collect::<BTreeMap<_, _>>();

        let mut log = event::LogEvent::from(fields);
        if let Some(metadata_value) = trace.metadata {
            if let Some(decoded_value) = decode_value(metadata_value) {
                *log.metadata_mut().value_mut() = decoded_value;
            }
        }

        Self::from(log)
    }
}

impl From<MetricValue> for event::MetricValue {
    fn from(value: MetricValue) -> Self {
        match value {
            MetricValue::Counter(counter) => Self::Counter {
                value: counter.value,
            },
            MetricValue::Gauge(gauge) => Self::Gauge { value: gauge.value },
            MetricValue::Set(set) => Self::Set {
                values: set.values.into_iter().collect(),
            },
            MetricValue::Distribution1(dist) => Self::Distribution {
                statistic: dist.statistic().into(),
                samples: event::metric::zip_samples(dist.values, dist.sample_rates),
            },
            MetricValue::Distribution2(dist) => Self::Distribution {
                statistic: dist.statistic().into(),
                samples: dist.samples.into_iter().map(Into::into).collect(),
            },
            MetricValue::AggregatedHistogram1(hist) => Self::AggregatedHistogram {
                buckets: event::metric::zip_buckets(
                    hist.buckets,
                    hist.counts.iter().map(|h| u64::from(*h)),
                ),
                count: u64::from(hist.count),
                sum: hist.sum,
            },
            MetricValue::AggregatedHistogram2(hist) => Self::AggregatedHistogram {
                buckets: hist.buckets.into_iter().map(Into::into).collect(),
                count: u64::from(hist.count),
                sum: hist.sum,
            },
            MetricValue::AggregatedHistogram3(hist) => Self::AggregatedHistogram {
                buckets: hist.buckets.into_iter().map(Into::into).collect(),
                count: hist.count,
                sum: hist.sum,
            },
            MetricValue::AggregatedSummary1(summary) => Self::AggregatedSummary {
                quantiles: event::metric::zip_quantiles(summary.quantiles, summary.values),
                count: u64::from(summary.count),
                sum: summary.sum,
            },
            MetricValue::AggregatedSummary2(summary) => Self::AggregatedSummary {
                quantiles: summary.quantiles.into_iter().map(Into::into).collect(),
                count: u64::from(summary.count),
                sum: summary.sum,
            },
            MetricValue::AggregatedSummary3(summary) => Self::AggregatedSummary {
                quantiles: summary.quantiles.into_iter().map(Into::into).collect(),
                count: summary.count,
                sum: summary.sum,
            },
            MetricValue::Sketch(sketch) => match sketch.sketch.unwrap() {
                sketch::Sketch::AgentDdSketch(ddsketch) => Self::Sketch {
                    sketch: ddsketch.into(),
                },
            },
        }
    }
}

impl From<Metric> for event::Metric {
    fn from(metric: Metric) -> Self {
        let kind = match metric.kind() {
            metric::Kind::Incremental => event::MetricKind::Incremental,
            metric::Kind::Absolute => event::MetricKind::Absolute,
        };

        let name = metric.name;

        let namespace = (!metric.namespace.is_empty()).then_some(metric.namespace);

        let timestamp = metric.timestamp.map(|ts| {
            chrono::Utc
                .timestamp_opt(ts.seconds, ts.nanos as u32)
                .single()
                .expect("invalid timestamp")
        });

        let mut tags = MetricTags(
            metric
                .tags_v2
                .into_iter()
                .map(|(tag, values)| {
                    (
                        tag,
                        values
                            .values
                            .into_iter()
                            .map(|value| event::metric::TagValue::from(value.value))
                            .collect(),
                    )
                })
                .collect(),
        );
        // The current Vector encoding includes copies of the "single" values of tags in `tags_v2`
        // above. This `extend` will re-add those values, forcing them to become the last added in
        // the value set.
        tags.extend(metric.tags_v1.into_iter());
        let tags = (!tags.is_empty()).then_some(tags);

        let value = event::MetricValue::from(metric.value.unwrap());

        let mut metadata = event::EventMetadata::default();
        if let Some(metadata_value) = metric.metadata {
            if let Some(decoded_value) = decode_value(metadata_value) {
                *metadata.value_mut() = decoded_value;
            }
        }

        Self::new_with_metadata(name, kind, value, metadata)
            .with_namespace(namespace)
            .with_tags(tags)
            .with_timestamp(timestamp)
            .with_interval_ms(std::num::NonZeroU32::new(metric.interval_ms))
    }
}

impl From<EventWrapper> for event::Event {
    fn from(proto: EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => Self::Log(proto.into()),
            Event::Metric(proto) => Self::Metric(proto.into()),
            Event::Trace(proto) => Self::Trace(proto.into()),
        }
    }
}

impl From<event::LogEvent> for Log {
    fn from(log_event: event::LogEvent) -> Self {
        WithMetadata::<Self>::from(log_event).data
    }
}

impl From<event::TraceEvent> for Trace {
    fn from(trace: event::TraceEvent) -> Self {
        WithMetadata::<Self>::from(trace).data
    }
}

impl From<event::LogEvent> for WithMetadata<Log> {
    fn from(log_event: event::LogEvent) -> Self {
        let (value, metadata) = log_event.into_parts();

        // Due to the backwards compatibility requirement by the
        // "event_can_go_from_raw_prost_to_eventarray_encodable" test, "fields" must not
        // be empty, since that will decode as an empty array. A "dummy" value is placed
        // in fields instead which is ignored during decoding. To reduce encoding bloat
        // from a dummy value, it is only used when the root value type is not an object.
        // Once this backwards compatibility is no longer required, "fields" can
        // be entirely removed from the Log object

        let data = if let VrlValue::Object(fields) = value {
            // using only "fields" to prevent having to use the dummy value
            Log {
                fields: fields
                    .into_iter()
                    .map(|(k, v)| (k, encode_value(v)))
                    .collect::<BTreeMap<_, _>>(),
                value: None,
                metadata: Some(encode_value(metadata.value().clone())),
            }
        } else {
            let mut dummy = BTreeMap::new();
            // must insert at least 1 field, otherwise it is emitted entirely.
            // this value is ignored in the decoding step (since value is provided)
            dummy.insert(".".to_owned(), encode_value(VrlValue::Null));
            Log {
                fields: dummy,
                value: Some(encode_value(value)),
                metadata: Some(encode_value(metadata.value().clone())),
            }
        };

        Self { data, metadata }
    }
}

impl From<event::TraceEvent> for WithMetadata<Trace> {
    fn from(trace: event::TraceEvent) -> Self {
        let (fields, metadata) = trace.into_parts();
        let fields = fields
            .into_iter()
            .map(|(k, v)| (k, encode_value(v)))
            .collect::<BTreeMap<_, _>>();

        let data = Trace {
            fields,
            metadata: Some(encode_value(metadata.value().clone())),
        };
        Self { data, metadata }
    }
}

impl From<event::Metric> for Metric {
    fn from(metric: event::Metric) -> Self {
        WithMetadata::<Self>::from(metric).data
    }
}

impl From<event::MetricValue> for MetricValue {
    fn from(value: event::MetricValue) -> Self {
        match value {
            event::MetricValue::Counter { value } => Self::Counter(Counter { value }),
            event::MetricValue::Gauge { value } => Self::Gauge(Gauge { value }),
            event::MetricValue::Set { values } => Self::Set(Set {
                values: values.into_iter().collect(),
            }),
            event::MetricValue::Distribution { samples, statistic } => {
                Self::Distribution2(Distribution2 {
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
            } => Self::AggregatedHistogram3(AggregatedHistogram3 {
                buckets: buckets.into_iter().map(Into::into).collect(),
                count,
                sum,
            }),
            event::MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => Self::AggregatedSummary3(AggregatedSummary3 {
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

                    Self::Sketch(Sketch {
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
        }
    }
}

impl From<event::Metric> for WithMetadata<Metric> {
    fn from(metric: event::Metric) -> Self {
        let (series, data, metadata) = metric.into_parts();
        let name = series.name.name;
        let namespace = series.name.namespace.unwrap_or_default();

        let timestamp = data.time.timestamp.map(|ts| prost_types::Timestamp {
            seconds: ts.timestamp(),
            nanos: ts.timestamp_subsec_nanos() as i32,
        });

        let interval_ms = data.time.interval_ms.map_or(0, std::num::NonZeroU32::get);

        let tags = series.tags.unwrap_or_default();

        let kind = match data.kind {
            event::MetricKind::Incremental => metric::Kind::Incremental,
            event::MetricKind::Absolute => metric::Kind::Absolute,
        }
        .into();

        let metric = MetricValue::from(data.value);

        // Include the "single" value of the tags in order to be forward-compatible with older
        // versions of Vector.
        let tags_v1 = tags
            .0
            .iter()
            .filter_map(|(tag, values)| {
                values
                    .as_single()
                    .map(|value| (tag.clone(), value.to_string()))
            })
            .collect();
        // These are the full tag values.
        let tags_v2 = tags
            .0
            .into_iter()
            .map(|(tag, values)| {
                let values = values
                    .into_iter()
                    .map(|value| TagValue {
                        value: value.into_option(),
                    })
                    .collect();
                (tag, TagValues { values })
            })
            .collect();

        let data = Metric {
            name,
            namespace,
            timestamp,
            tags_v1,
            tags_v2,
            kind,
            interval_ms,
            value: Some(metric),
            metadata: Some(encode_value(metadata.value().clone())),
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
            event::Event::Trace(trace) => WithMetadata::<Trace>::from(trace).into(),
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
                    .unwrap_or(if pos { i16::MAX } else { i16::MIN })
            })
            .collect::<Vec<_>>();
        let counts = sketch
            .n
            .into_iter()
            .map(|n| n.try_into().unwrap_or(u16::MAX))
            .collect::<Vec<_>>();
        MetricSketch::AgentDDSketch(
            AgentDDSketch::from_raw(
                sketch.count,
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
            chrono::Utc
                .timestamp_opt(ts.seconds, ts.nanos as u32)
                .single()
                .expect("invalid timestamp"),
        )),
        Some(value::Kind::Integer(value)) => Some(event::Value::Integer(value)),
        Some(value::Kind::Float(value)) => Some(event::Value::Float(NotNan::new(value).unwrap())),
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
    fields
        .into_iter()
        .map(|(key, value)| decode_value(value).map(|value| (key, value)))
        .collect::<Option<BTreeMap<_, _>>>()
        .map(event::Value::Object)
}

fn decode_array(items: Vec<Value>) -> Option<event::Value> {
    items
        .into_iter()
        .map(decode_value)
        .collect::<Option<Vec<_>>>()
        .map(event::Value::Array)
}

fn encode_value(value: event::Value) -> Value {
    Value {
        kind: match value {
            event::Value::Bytes(b) => Some(value::Kind::RawBytes(b)),
            event::Value::Regex(regex) => Some(value::Kind::RawBytes(regex.as_bytes())),
            event::Value::Timestamp(ts) => Some(value::Kind::Timestamp(prost_types::Timestamp {
                seconds: ts.timestamp(),
                nanos: ts.timestamp_subsec_nanos() as i32,
            })),
            event::Value::Integer(value) => Some(value::Kind::Integer(value)),
            event::Value::Float(value) => Some(value::Kind::Float(value.into_inner())),
            event::Value::Boolean(value) => Some(value::Kind::Boolean(value)),
            event::Value::Object(fields) => Some(value::Kind::Map(encode_map(fields))),
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

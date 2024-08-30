use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::TimeZone;
use ordered_float::NotNan;
use uuid::Uuid;

use super::{MetricTags, WithMetadata};
use crate::{event, metrics::AgentDDSketch};

#[allow(warnings, clippy::all, clippy::pedantic)]
mod proto_event {
    include!(concat!(env!("OUT_DIR"), "/event.rs"));
}
pub use event_wrapper::Event;
pub use metric::Value as MetricValue;
pub use proto_event::*;
use vrl::value::{ObjectMap, Value as VrlValue};

use super::{array, metric::MetricSketch, EventMetadata};

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

impl From<Log> for super::LogEvent {
    fn from(log: Log) -> Self {
        #[allow(deprecated)]
        let metadata = log
            .metadata_full
            .map(Into::into)
            .or_else(|| {
                log.metadata
                    .and_then(decode_value)
                    .map(EventMetadata::default_with_value)
            })
            .unwrap_or_default();

        if let Some(value) = log.value {
            Self::from_parts(decode_value(value).unwrap_or(VrlValue::Null), metadata)
        } else {
            // This is for backwards compatibility. Only `value` should be set
            let fields = log
                .fields
                .into_iter()
                .filter_map(|(k, v)| decode_value(v).map(|value| (k.into(), value)))
                .collect::<ObjectMap>();

            Self::from_map(fields, metadata)
        }
    }
}

impl From<Trace> for super::TraceEvent {
    fn from(trace: Trace) -> Self {
        #[allow(deprecated)]
        let metadata = trace
            .metadata_full
            .map(Into::into)
            .or_else(|| {
                trace
                    .metadata
                    .and_then(decode_value)
                    .map(EventMetadata::default_with_value)
            })
            .unwrap_or_default();

        let fields = trace
            .fields
            .into_iter()
            .filter_map(|(k, v)| decode_value(v).map(|value| (k.into(), value)))
            .collect::<ObjectMap>();

        Self::from(super::LogEvent::from_map(fields, metadata))
    }
}

impl From<MetricValue> for super::MetricValue {
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
                samples: super::metric::zip_samples(dist.values, dist.sample_rates),
            },
            MetricValue::Distribution2(dist) => Self::Distribution {
                statistic: dist.statistic().into(),
                samples: dist.samples.into_iter().map(Into::into).collect(),
            },
            MetricValue::AggregatedHistogram1(hist) => Self::AggregatedHistogram {
                buckets: super::metric::zip_buckets(
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
                quantiles: super::metric::zip_quantiles(summary.quantiles, summary.values),
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

impl From<Metric> for super::Metric {
    fn from(metric: Metric) -> Self {
        let kind = match metric.kind() {
            metric::Kind::Incremental => super::MetricKind::Incremental,
            metric::Kind::Absolute => super::MetricKind::Absolute,
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
                            .map(|value| super::metric::TagValue::from(value.value))
                            .collect(),
                    )
                })
                .collect(),
        );
        // The current Vector encoding includes copies of the "single" values of tags in `tags_v2`
        // above. This `extend` will re-add those values, forcing them to become the last added in
        // the value set.
        tags.extend(metric.tags_v1);
        let tags = (!tags.is_empty()).then_some(tags);

        let value = super::MetricValue::from(metric.value.unwrap());

        #[allow(deprecated)]
        let metadata = metric
            .metadata_full
            .map(Into::into)
            .or_else(|| {
                metric
                    .metadata
                    .and_then(decode_value)
                    .map(EventMetadata::default_with_value)
            })
            .unwrap_or_default();

        Self::new_with_metadata(name, kind, value, metadata)
            .with_namespace(namespace)
            .with_tags(tags)
            .with_timestamp(timestamp)
            .with_interval_ms(std::num::NonZeroU32::new(metric.interval_ms))
    }
}

impl From<EventWrapper> for super::Event {
    fn from(proto: EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            Event::Log(proto) => Self::Log(proto.into()),
            Event::Metric(proto) => Self::Metric(proto.into()),
            Event::Trace(proto) => Self::Trace(proto.into()),
        }
    }
}

impl From<super::LogEvent> for Log {
    fn from(log_event: super::LogEvent) -> Self {
        WithMetadata::<Self>::from(log_event).data
    }
}

impl From<super::TraceEvent> for Trace {
    fn from(trace: super::TraceEvent) -> Self {
        WithMetadata::<Self>::from(trace).data
    }
}

impl From<super::LogEvent> for WithMetadata<Log> {
    fn from(log_event: super::LogEvent) -> Self {
        let (value, metadata) = log_event.into_parts();

        // Due to the backwards compatibility requirement by the
        // "event_can_go_from_raw_prost_to_eventarray_encodable" test, "fields" must not
        // be empty, since that will decode as an empty array. A "dummy" value is placed
        // in fields instead which is ignored during decoding. To reduce encoding bloat
        // from a dummy value, it is only used when the root value type is not an object.
        // Once this backwards compatibility is no longer required, "fields" can
        // be entirely removed from the Log object
        let (fields, value) = if let VrlValue::Object(fields) = value {
            // using only "fields" to prevent having to use the dummy value
            let fields = fields
                .into_iter()
                .map(|(k, v)| (k.into(), encode_value(v)))
                .collect::<BTreeMap<_, _>>();

            (fields, None)
        } else {
            // Must insert at least one field, otherwise the field is omitted entirely on the
            // Protocol Buffers side. The dummy field value is ultimately ignored in the decoding
            // step since `value` is provided.
            let mut dummy_fields = BTreeMap::new();
            dummy_fields.insert(".".to_owned(), encode_value(VrlValue::Null));

            (dummy_fields, Some(encode_value(value)))
        };

        #[allow(deprecated)]
        let data = Log {
            fields,
            value,
            metadata: Some(encode_value(metadata.value().clone())),
            metadata_full: Some(metadata.clone().into()),
        };

        Self { data, metadata }
    }
}

impl From<super::TraceEvent> for WithMetadata<Trace> {
    fn from(trace: super::TraceEvent) -> Self {
        let (fields, metadata) = trace.into_parts();
        let fields = fields
            .into_iter()
            .map(|(k, v)| (k.into(), encode_value(v)))
            .collect::<BTreeMap<_, _>>();

        #[allow(deprecated)]
        let data = Trace {
            fields,
            metadata: Some(encode_value(metadata.value().clone())),
            metadata_full: Some(metadata.clone().into()),
        };

        Self { data, metadata }
    }
}

impl From<super::Metric> for Metric {
    fn from(metric: super::Metric) -> Self {
        WithMetadata::<Self>::from(metric).data
    }
}

impl From<super::MetricValue> for MetricValue {
    fn from(value: super::MetricValue) -> Self {
        match value {
            super::MetricValue::Counter { value } => Self::Counter(Counter { value }),
            super::MetricValue::Gauge { value } => Self::Gauge(Gauge { value }),
            super::MetricValue::Set { values } => Self::Set(Set {
                values: values.into_iter().collect(),
            }),
            super::MetricValue::Distribution { samples, statistic } => {
                Self::Distribution2(Distribution2 {
                    samples: samples.into_iter().map(Into::into).collect(),
                    statistic: match statistic {
                        super::StatisticKind::Histogram => StatisticKind::Histogram,
                        super::StatisticKind::Summary => StatisticKind::Summary,
                    }
                    .into(),
                })
            }
            super::MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => Self::AggregatedHistogram3(AggregatedHistogram3 {
                buckets: buckets.into_iter().map(Into::into).collect(),
                count,
                sum,
            }),
            super::MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => Self::AggregatedSummary3(AggregatedSummary3 {
                quantiles: quantiles.into_iter().map(Into::into).collect(),
                count,
                sum,
            }),
            super::MetricValue::Sketch { sketch } => match sketch {
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

impl From<super::Metric> for WithMetadata<Metric> {
    fn from(metric: super::Metric) -> Self {
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
            super::MetricKind::Incremental => metric::Kind::Incremental,
            super::MetricKind::Absolute => metric::Kind::Absolute,
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

        #[allow(deprecated)]
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
            metadata_full: Some(metadata.clone().into()),
        };

        Self { data, metadata }
    }
}

impl From<super::Event> for Event {
    fn from(event: super::Event) -> Self {
        WithMetadata::<Self>::from(event).data
    }
}

impl From<super::Event> for WithMetadata<Event> {
    fn from(event: super::Event) -> Self {
        match event {
            super::Event::Log(log_event) => WithMetadata::<Log>::from(log_event).into(),
            super::Event::Metric(metric) => WithMetadata::<Metric>::from(metric).into(),
            super::Event::Trace(trace) => WithMetadata::<Trace>::from(trace).into(),
        }
    }
}

impl From<super::Event> for EventWrapper {
    fn from(event: super::Event) -> Self {
        WithMetadata::<EventWrapper>::from(event).data
    }
}

impl From<super::Event> for WithMetadata<EventWrapper> {
    fn from(event: super::Event) -> Self {
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

impl From<super::metadata::Secrets> for Secrets {
    fn from(value: super::metadata::Secrets) -> Self {
        Self {
            entries: value.into_iter().map(|(k, v)| (k, v.to_string())).collect(),
        }
    }
}

impl From<Secrets> for super::metadata::Secrets {
    fn from(value: Secrets) -> Self {
        let mut secrets = Self::new();
        for (k, v) in value.entries {
            secrets.insert(k, v);
        }

        secrets
    }
}

impl From<super::DatadogMetricOriginMetadata> for DatadogOriginMetadata {
    fn from(value: super::DatadogMetricOriginMetadata) -> Self {
        Self {
            origin_product: value.product(),
            origin_category: value.category(),
            origin_service: value.service(),
        }
    }
}

impl From<DatadogOriginMetadata> for super::DatadogMetricOriginMetadata {
    fn from(value: DatadogOriginMetadata) -> Self {
        Self::new(
            value.origin_product,
            value.origin_category,
            value.origin_service,
        )
    }
}

impl From<crate::config::OutputId> for OutputId {
    fn from(value: crate::config::OutputId) -> Self {
        Self {
            component: value.component.into_id(),
            port: value.port,
        }
    }
}

impl From<OutputId> for crate::config::OutputId {
    fn from(value: OutputId) -> Self {
        Self::from((value.component, value.port))
    }
}

impl From<EventMetadata> for Metadata {
    fn from(value: EventMetadata) -> Self {
        let super::metadata::Inner {
            value,
            secrets,
            source_id,
            source_type,
            upstream_id,
            datadog_origin_metadata,
            source_event_id,
            ..
        } = value.into_owned();

        let secrets = (!secrets.is_empty()).then(|| secrets.into());

        Self {
            value: Some(encode_value(value)),
            datadog_origin_metadata: datadog_origin_metadata.map(Into::into),
            source_id: source_id.map(|s| s.to_string()),
            source_type: source_type.map(|s| s.to_string()),
            upstream_id: upstream_id.map(|id| id.as_ref().clone()).map(Into::into),
            secrets,
            source_event_id: source_event_id.into(),
        }
    }
}

impl From<Metadata> for EventMetadata {
    fn from(value: Metadata) -> Self {
        let mut metadata = EventMetadata::default();

        if let Some(value) = value.value.and_then(decode_value) {
            *metadata.value_mut() = value;
        }

        if let Some(source_id) = value.source_id {
            metadata.set_source_id(Arc::new(source_id.into()));
        }

        if let Some(source_type) = value.source_type {
            metadata.set_source_type(source_type);
        }

        if let Some(upstream_id) = value.upstream_id {
            metadata.set_upstream_id(Arc::new(upstream_id.into()));
        }

        if let Some(secrets) = value.secrets {
            metadata.secrets_mut().merge(secrets.into());
        }

        if let Some(origin_metadata) = value.datadog_origin_metadata {
            metadata = metadata.with_origin_metadata(origin_metadata.into());
        }

        if let Ok(uuid) = Uuid::from_slice(&value.source_event_id) {
            metadata = metadata.with_source_event_id(uuid);
        } else {
            error!("Invalid source_event_id in metadata");
        }

        metadata
    }
}

fn decode_value(input: Value) -> Option<super::Value> {
    match input.kind {
        Some(value::Kind::RawBytes(data)) => Some(super::Value::Bytes(data)),
        Some(value::Kind::Timestamp(ts)) => Some(super::Value::Timestamp(
            chrono::Utc
                .timestamp_opt(ts.seconds, ts.nanos as u32)
                .single()
                .expect("invalid timestamp"),
        )),
        Some(value::Kind::Integer(value)) => Some(super::Value::Integer(value)),
        Some(value::Kind::Float(value)) => Some(super::Value::Float(NotNan::new(value).unwrap())),
        Some(value::Kind::Boolean(value)) => Some(super::Value::Boolean(value)),
        Some(value::Kind::Map(map)) => decode_map(map.fields),
        Some(value::Kind::Array(array)) => decode_array(array.items),
        Some(value::Kind::Null(_)) => Some(super::Value::Null),
        None => {
            error!("Encoded event contains unknown value kind.");
            None
        }
    }
}

fn decode_map(fields: BTreeMap<String, Value>) -> Option<super::Value> {
    fields
        .into_iter()
        .map(|(key, value)| decode_value(value).map(|value| (key.into(), value)))
        .collect::<Option<ObjectMap>>()
        .map(event::Value::Object)
}

fn decode_array(items: Vec<Value>) -> Option<super::Value> {
    items
        .into_iter()
        .map(decode_value)
        .collect::<Option<Vec<_>>>()
        .map(super::Value::Array)
}

fn encode_value(value: super::Value) -> Value {
    Value {
        kind: match value {
            super::Value::Bytes(b) => Some(value::Kind::RawBytes(b)),
            super::Value::Regex(regex) => Some(value::Kind::RawBytes(regex.as_bytes())),
            super::Value::Timestamp(ts) => Some(value::Kind::Timestamp(prost_types::Timestamp {
                seconds: ts.timestamp(),
                nanos: ts.timestamp_subsec_nanos() as i32,
            })),
            super::Value::Integer(value) => Some(value::Kind::Integer(value)),
            super::Value::Float(value) => Some(value::Kind::Float(value.into_inner())),
            super::Value::Boolean(value) => Some(value::Kind::Boolean(value)),
            super::Value::Object(fields) => Some(value::Kind::Map(encode_map(fields))),
            super::Value::Array(items) => Some(value::Kind::Array(encode_array(items))),
            super::Value::Null => Some(value::Kind::Null(ValueNull::NullValue as i32)),
        },
    }
}

fn encode_map(fields: ObjectMap) -> ValueMap {
    ValueMap {
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.into(), encode_value(value)))
            .collect(),
    }
}

fn encode_array(items: Vec<super::Value>) -> ValueArray {
    ValueArray {
        items: items.into_iter().map(encode_value).collect(),
    }
}

use std::{collections::BTreeMap, sync::Arc};

use chrono::TimeZone;
use ordered_float::NotNan;
use snafu::Snafu;
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

use super::EventFinalizers;
use super::metadata::{Inner, default_schema_definition};
use super::{EventMetadata, array, metric::MetricSketch};

/// Failure converting a structurally valid internal event protobuf into Vector's in-memory types.
///
/// Distinct from a `prost` decode failure: the bytes parsed as protobuf, but a required event
/// variant was absent/unrecognized or a value could not be represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Snafu)]
pub enum EventProtoError {
    #[snafu(display(
        "event protobuf was structurally valid but an event or metric variant was absent or unrecognized; this often indicates a version mismatch"
    ))]
    UnrecognizedEventVariant,
    #[snafu(display(
        "event protobuf contained a NaN float, which cannot be represented in Vector's event model"
    ))]
    NanFloat,
    #[snafu(display("event protobuf contained an invalid timestamp"))]
    InvalidTimestamp,
}

fn require_variant<T>(value: Option<T>) -> Result<T, EventProtoError> {
    value.ok_or(EventProtoError::UnrecognizedEventVariant)
}

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

impl TryFrom<EventArray> for array::EventArray {
    type Error = EventProtoError;

    fn try_from(events: EventArray) -> Result<Self, Self::Error> {
        match require_variant(events.events)? {
            event_array::Events::Logs(logs) => Ok(Self::Logs(
                logs.logs
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            )),
            event_array::Events::Metrics(metrics) => Ok(Self::Metrics(
                metrics
                    .metrics
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            )),
            event_array::Events::Traces(traces) => Ok(Self::Traces(
                traces
                    .traces
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            )),
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

impl TryFrom<Log> for super::LogEvent {
    type Error = EventProtoError;

    #[allow(deprecated)]
    fn try_from(log: Log) -> Result<Self, Self::Error> {
        let metadata = decode_event_metadata(log.metadata_full, log.metadata)?;

        if let Some(value) = log.value {
            Ok(Self::from_parts(
                decode_value(value)?.unwrap_or(VrlValue::Null),
                metadata,
            ))
        } else {
            // This is for backwards compatibility. Only `value` should be set
            let mut fields = ObjectMap::new();
            for (k, v) in log.fields {
                if let Some(value) = decode_value(v)? {
                    fields.insert(k.into(), value);
                }
            }

            Ok(Self::from_map(fields, metadata))
        }
    }
}

impl TryFrom<Trace> for super::TraceEvent {
    type Error = EventProtoError;

    fn try_from(trace: Trace) -> Result<Self, Self::Error> {
        #[allow(deprecated)]
        let metadata = decode_event_metadata(trace.metadata_full, trace.metadata)?;

        let mut fields = ObjectMap::new();
        for (k, v) in trace.fields {
            if let Some(value) = decode_value(v)? {
                fields.insert(k.into(), value);
            }
        }

        Ok(Self::from(super::LogEvent::from_map(fields, metadata)))
    }
}

impl TryFrom<MetricValue> for super::MetricValue {
    type Error = EventProtoError;

    #[allow(deprecated)]
    fn try_from(value: MetricValue) -> Result<Self, Self::Error> {
        Ok(match value {
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
            MetricValue::Sketch(sketch) => match require_variant(sketch.sketch)? {
                sketch::Sketch::AgentDdSketch(ddsketch) => Self::Sketch {
                    sketch: ddsketch.into(),
                },
            },
        })
    }
}

impl TryFrom<Metric> for super::Metric {
    type Error = EventProtoError;

    #[allow(deprecated)]
    fn try_from(metric: Metric) -> Result<Self, Self::Error> {
        let kind = match metric.kind() {
            metric::Kind::Incremental => super::MetricKind::Incremental,
            metric::Kind::Absolute => super::MetricKind::Absolute,
        };

        let name = metric.name;

        let namespace = (!metric.namespace.is_empty()).then_some(metric.namespace);

        let timestamp = metric
            .timestamp
            .as_ref()
            .map(decode_timestamp)
            .transpose()?;

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
        // above. Only re-add a v1 value when it disagrees with v2; inserting an already-selected
        // value would reorder an otherwise canonical enhanced tag set.
        for (tag, value) in metric.tags_v1 {
            if tags.get(&tag) != Some(value.as_str()) {
                tags.insert(tag, value);
            }
        }
        let tags = (!tags.is_empty()).then_some(tags);

        let value = require_variant(metric.value)?.try_into()?;

        let metadata = decode_event_metadata(metric.metadata_full, metric.metadata)?;

        Ok(Self::new_with_metadata(name, kind, value, metadata)
            .with_namespace(namespace)
            .with_tags(tags)
            .with_timestamp(timestamp)
            .with_interval_ms(std::num::NonZeroU32::new(metric.interval_ms)))
    }
}

impl TryFrom<EventWrapper> for super::Event {
    type Error = EventProtoError;

    fn try_from(proto: EventWrapper) -> Result<Self, Self::Error> {
        match require_variant(proto.event)? {
            Event::Log(proto) => Ok(Self::Log(proto.try_into()?)),
            Event::Metric(proto) => Ok(Self::Metric(proto.try_into()?)),
            Event::Trace(proto) => Ok(Self::Trace(proto.try_into()?)),
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

        // Value never wraps as timestamp_subsec_nanos returns a value <= 1_999_999_999
        // (as per chrono leap-second specs), which is below i32::MAX
        #[allow(clippy::cast_possible_wrap)]
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
            source_event_id: source_event_id.map_or(vec![], std::convert::Into::into),
        }
    }
}

impl TryFrom<Metadata> for EventMetadata {
    type Error = EventProtoError;

    fn try_from(value: Metadata) -> Result<Self, Self::Error> {
        let Metadata {
            value: metadata_value,
            source_id,
            source_type,
            upstream_id,
            secrets,
            datadog_origin_metadata,
            source_event_id,
        } = value;

        let metadata_value = match metadata_value {
            Some(value) => decode_value(value)?,
            None => None,
        };
        let source_id = source_id.map(|s| Arc::new(s.into()));
        let upstream_id = upstream_id.map(|id| Arc::new(id.into()));
        let secrets = secrets.map(Into::into);
        let datadog_origin_metadata = datadog_origin_metadata.map(Into::into);
        let source_event_id = if source_event_id.is_empty() {
            None
        } else {
            match Uuid::from_slice(&source_event_id) {
                Ok(id) => Some(id),
                Err(error) => {
                    error!(
                        %error,
                        source_event_id = %String::from_utf8_lossy(&source_event_id),
                        "Failed to parse source_event_id.",
                    );
                    None
                }
            }
        };

        Ok(EventMetadata {
            inner: Arc::new(Inner {
                value: metadata_value
                    .unwrap_or_else(|| vrl::value::Value::Object(ObjectMap::new())),
                secrets: secrets.unwrap_or_default(),
                finalizers: EventFinalizers::default(),
                source_id,
                source_type: source_type.map(Into::into),
                upstream_id,
                schema_definition: default_schema_definition(),
                dropped_fields: ObjectMap::new(),
                datadog_origin_metadata,
                source_event_id,
            }),
            last_transform_timestamp: None,
        })
    }
}

fn decode_event_metadata(
    metadata_full: Option<Metadata>,
    metadata: Option<Value>,
) -> Result<EventMetadata, EventProtoError> {
    if let Some(full) = metadata_full {
        full.try_into()
    } else if let Some(value) = metadata {
        Ok(decode_value(value)?
            .map(EventMetadata::default_with_value)
            .unwrap_or_default())
    } else {
        Ok(EventMetadata::default())
    }
}

fn decode_timestamp(
    ts: &prost_types::Timestamp,
) -> Result<chrono::DateTime<chrono::Utc>, EventProtoError> {
    // Sign is never lost as ts.nanos is always non negative (per proto spec)
    #[allow(clippy::cast_sign_loss)]
    chrono::Utc
        .timestamp_opt(ts.seconds, ts.nanos as u32)
        .single()
        .ok_or(EventProtoError::InvalidTimestamp)
}

fn decode_value(input: Value) -> Result<Option<super::Value>, EventProtoError> {
    match input.kind {
        Some(value::Kind::RawBytes(data)) => Ok(Some(super::Value::Bytes(data))),
        Some(value::Kind::Timestamp(ts)) => {
            Ok(Some(super::Value::Timestamp(decode_timestamp(&ts)?)))
        }
        Some(value::Kind::Integer(value)) => Ok(Some(super::Value::Integer(value))),
        Some(value::Kind::Float(value)) => {
            let value = NotNan::new(value).map_err(|_| EventProtoError::NanFloat)?;
            Ok(Some(super::Value::Float(value)))
        }
        Some(value::Kind::Boolean(value)) => Ok(Some(super::Value::Boolean(value))),
        Some(value::Kind::Map(map)) => decode_map(map.fields),
        Some(value::Kind::Array(array)) => decode_array(array.items),
        Some(value::Kind::Null(_)) => Ok(Some(super::Value::Null)),
        None => {
            error!("Encoded event contains unknown value kind.");
            Ok(None)
        }
    }
}

fn decode_map(fields: BTreeMap<String, Value>) -> Result<Option<super::Value>, EventProtoError> {
    let mut map = ObjectMap::new();
    for (key, value) in fields {
        let Some(decoded) = decode_value(value)? else {
            return Ok(None);
        };
        map.insert(key.into(), decoded);
    }
    Ok(Some(event::Value::Object(map)))
}

fn decode_array(items: Vec<Value>) -> Result<Option<super::Value>, EventProtoError> {
    let mut decoded_items = Vec::with_capacity(items.len());
    for item in items {
        let Some(decoded) = decode_value(item)? else {
            return Ok(None);
        };
        decoded_items.push(decoded);
    }
    Ok(Some(super::Value::Array(decoded_items)))
}

fn encode_value(value: super::Value) -> Value {
    Value {
        kind: match value {
            super::Value::Bytes(b) => Some(value::Kind::RawBytes(b)),
            super::Value::Regex(regex) => Some(value::Kind::RawBytes(regex.as_bytes())),
            // Value never wraps as timestamp_subsec_nanos returns a value <= 1_999_999_999
            // (as per chrono leap-second specs), which is below i32::MAX
            #[allow(clippy::cast_possible_wrap)]
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

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use super::*;
    use crate::event::{MetricValue as EventMetricValue, metric};

    // These payloads are frozen to pin legacy protobuf field numbers on the wire. Do not
    // regenerate them from the current Rust types; each payload's exact contents are documented
    // below and mirrored by its test assertions.

    // EventArray.metrics containing four pre-v24 metrics:
    // - AggregatedHistogram1 (field 9): bucket 1.5/count 2, total count 2, sum 3.0.
    // - AggregatedHistogram2 (field 13): bucket 1.5/count 2, total count 2, sum 3.0.
    // - AggregatedSummary1 (field 10): quantile 0.5/value 1.5, count 2, sum 3.0.
    // - AggregatedSummary2 (field 14): quantile 0.5/value 1.5, count 2, sum 3.0.
    // These variants use the legacy u32 count representation.
    const PRE_V24_METRICS: &[u8] = &[
        18, 170, 1, 10, 38, 10, 10, 104, 105, 115, 116, 111, 103, 114, 97, 109, 49, 74, 24, 10, 8,
        0, 0, 0, 0, 0, 0, 248, 63, 18, 1, 2, 24, 2, 33, 0, 0, 0, 0, 0, 0, 8, 64, 10, 38, 10, 10,
        104, 105, 115, 116, 111, 103, 114, 97, 109, 50, 106, 24, 10, 11, 9, 0, 0, 0, 0, 0, 0, 248,
        63, 16, 2, 16, 2, 25, 0, 0, 0, 0, 0, 0, 8, 64, 10, 43, 10, 8, 115, 117, 109, 109, 97, 114,
        121, 49, 82, 31, 10, 8, 0, 0, 0, 0, 0, 0, 224, 63, 18, 8, 0, 0, 0, 0, 0, 0, 248, 63, 24, 2,
        33, 0, 0, 0, 0, 0, 0, 8, 64, 10, 43, 10, 8, 115, 117, 109, 109, 97, 114, 121, 50, 114, 31,
        10, 18, 9, 0, 0, 0, 0, 0, 0, 224, 63, 17, 0, 0, 0, 0, 0, 0, 248, 63, 16, 2, 25, 0, 0, 0, 0,
        0, 0, 8, 64,
    ];

    // Pre-v27 Metric named "requests" with tags_v1 field 3 set to service="api" and a
    // Counter value of 1.0. This pins the legacy single-valued metric-tag representation.
    const PRE_V27_TAGS: &[u8] = &[
        10, 8, 114, 101, 113, 117, 101, 115, 116, 115, 26, 14, 10, 7, 115, 101, 114, 118, 105, 99,
        101, 18, 3, 97, 112, 105, 42, 9, 9, 0, 0, 0, 0, 0, 0, 240, 63,
    ];

    // Pre-v34 Log with deprecated metadata field 3 containing the bytes "legacy metadata".
    const PRE_V34_LOG_METADATA: &[u8] = &[
        26, 17, 10, 15, 108, 101, 103, 97, 99, 121, 32, 109, 101, 116, 97, 100, 97, 116, 97,
    ];

    // Pre-v34 Trace with deprecated metadata field 2 containing the bytes "legacy metadata".
    const PRE_V34_TRACE_METADATA: &[u8] = &[
        18, 17, 10, 15, 108, 101, 103, 97, 99, 121, 32, 109, 101, 116, 97, 100, 97, 116, 97,
    ];

    // Pre-v34 Metric named "requests" with a Counter value of 1.0 and deprecated metadata
    // field 19 containing the bytes "legacy metadata".
    const PRE_V34_METRIC_METADATA: &[u8] = &[
        10, 8, 114, 101, 113, 117, 101, 115, 116, 115, 42, 9, 9, 0, 0, 0, 0, 0, 0, 240, 63, 154, 1,
        17, 10, 15, 108, 101, 103, 97, 99, 121, 32, 109, 101, 116, 97, 100, 97, 116, 97,
    ];

    // Pre-v41 Metadata with source_type field 4 set to "legacy" and no source_event_id field 7.
    // This pins the expected default when decoding payloads created before event IDs existed.
    const PRE_V41_METADATA: &[u8] = &[34, 6, 108, 101, 103, 97, 99, 121];

    #[test]
    fn decodes_pre_v24_histogram_and_summary_variants() {
        let expected = [
            EventMetricValue::AggregatedHistogram {
                buckets: vec![metric::Bucket {
                    upper_limit: 1.5,
                    count: 2,
                }],
                count: 2,
                sum: 3.0,
            },
            EventMetricValue::AggregatedHistogram {
                buckets: vec![metric::Bucket {
                    upper_limit: 1.5,
                    count: 2,
                }],
                count: 2,
                sum: 3.0,
            },
            EventMetricValue::AggregatedSummary {
                quantiles: vec![metric::Quantile {
                    quantile: 0.5,
                    value: 1.5,
                }],
                count: 2,
                sum: 3.0,
            },
            EventMetricValue::AggregatedSummary {
                quantiles: vec![metric::Quantile {
                    quantile: 0.5,
                    value: 1.5,
                }],
                count: 2,
                sum: 3.0,
            },
        ];

        let encoded = EventArray::decode(PRE_V24_METRICS).unwrap();
        let Some(event_array::Events::Metrics(metrics)) = encoded.events else {
            panic!("legacy payload did not contain metrics");
        };
        let decoded = metrics
            .metrics
            .into_iter()
            .map(|metric| {
                crate::event::Metric::try_from(metric).expect("legacy metric should decode")
            })
            .map(|metric| metric.value().clone())
            .collect::<Vec<_>>();

        assert_eq!(decoded, expected);
    }

    #[test]
    fn decodes_pre_v27_single_valued_metric_tags() {
        let encoded = Metric::decode(PRE_V27_TAGS).unwrap();

        let decoded =
            crate::event::Metric::try_from(encoded).expect("legacy metric tags should decode");

        assert_eq!(decoded.tag_value("service").as_deref(), Some("api"));
    }

    #[test]
    fn current_metric_tags_preserve_enhanced_value_order() {
        let mut tags = metric::MetricTags::default();
        tags.set_multi_value(
            "service".to_owned(),
            [
                metric::TagValue::Value(String::new()),
                metric::TagValue::Bare,
            ],
        );
        let event = crate::event::Metric::new(
            "requests",
            crate::event::MetricKind::Absolute,
            EventMetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags));

        let decoded = crate::event::Metric::try_from(Metric::from(event))
            .expect("encoded metric should decode");
        let values = decoded
            .tags()
            .unwrap()
            .iter_all()
            .map(|(_, value)| value.map(str::to_owned))
            .collect::<Vec<_>>();

        assert_eq!(values, [Some(String::new()), None]);
    }

    #[test]
    #[allow(deprecated)]
    fn decodes_pre_v34_metadata_for_all_event_types() {
        let expected = VrlValue::from("legacy metadata");

        let log = crate::event::LogEvent::try_from(Log::decode(PRE_V34_LOG_METADATA).unwrap())
            .expect("legacy log metadata should decode");
        let trace =
            crate::event::TraceEvent::try_from(Trace::decode(PRE_V34_TRACE_METADATA).unwrap())
                .expect("legacy trace metadata should decode");
        let metric =
            crate::event::Metric::try_from(Metric::decode(PRE_V34_METRIC_METADATA).unwrap())
                .expect("legacy metric metadata should decode");

        assert_eq!(log.metadata().value(), &expected);
        assert_eq!(trace.metadata().value(), &expected);
        assert_eq!(metric.metadata().value(), &expected);
    }

    #[test]
    fn decodes_pre_v41_metadata_without_source_event_id() {
        let decoded = EventMetadata::try_from(Metadata::decode(PRE_V41_METADATA).unwrap())
            .expect("legacy metadata should decode");

        assert_eq!(decoded.source_event_id(), None);
        assert_eq!(decoded.source_type(), Some("legacy"));
    }

    #[test]
    fn missing_event_array_variant_is_an_error() {
        let proto = EventArray { events: None };
        assert_eq!(
            array::EventArray::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }

    #[test]
    fn missing_event_wrapper_variant_is_an_error() {
        let proto = EventWrapper { event: None };
        assert_eq!(
            crate::event::Event::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }

    #[test]
    fn missing_metric_value_variant_is_an_error() {
        let proto = Metric {
            name: "requests".into(),
            value: None,
            ..Metric::default()
        };
        assert_eq!(
            crate::event::Metric::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }

    #[test]
    fn missing_sketch_variant_is_an_error() {
        let proto = Metric {
            name: "requests".into(),
            value: Some(MetricValue::Sketch(Sketch { sketch: None })),
            ..Metric::default()
        };
        assert_eq!(
            crate::event::Metric::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }

    #[test]
    fn nan_float_value_is_an_error() {
        let value = Value {
            kind: Some(value::Kind::Float(f64::NAN)),
        };
        assert_eq!(decode_value(value), Err(EventProtoError::NanFloat));
    }

    #[test]
    fn nan_float_in_event_data_rejects_the_record() {
        let proto = EventWrapper {
            event: Some(Event::Log(Log {
                value: Some(Value {
                    kind: Some(value::Kind::Float(f64::NAN)),
                }),
                ..Log::default()
            })),
        };
        assert_eq!(
            crate::event::Event::try_from(proto),
            Err(EventProtoError::NanFloat)
        );
    }

    #[test]
    fn nan_float_in_metadata_rejects_the_record() {
        let proto = EventWrapper {
            event: Some(Event::Log(Log {
                metadata_full: Some(Metadata {
                    value: Some(Value {
                        kind: Some(value::Kind::Float(f64::NAN)),
                    }),
                    ..Metadata::default()
                }),
                ..Log::default()
            })),
        };
        assert_eq!(
            crate::event::Event::try_from(proto),
            Err(EventProtoError::NanFloat)
        );
    }

    #[test]
    fn unknown_event_array_oneof_tag_is_unrecognized_variant() {
        // Field 4 is not a member of `EventArray.events` (logs=1, metrics=2, traces=3).
        // Tag = (4 << 3) | 2 (length-delimited).
        let bytes = bytes::Bytes::from_static(&[34, 0]);
        let proto = EventArray::decode(bytes).expect("unknown field is valid protobuf");
        assert!(proto.events.is_none());
        assert_eq!(
            array::EventArray::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }

    #[test]
    fn unknown_event_wrapper_oneof_tag_is_unrecognized_variant() {
        // Field 4 is not a member of `EventWrapper.event` (log=1, metric=2, trace=3).
        let bytes = bytes::Bytes::from_static(&[34, 0]);
        let proto = EventWrapper::decode(bytes).expect("unknown field is valid protobuf");
        assert!(proto.event.is_none());
        assert_eq!(
            crate::event::Event::try_from(proto),
            Err(EventProtoError::UnrecognizedEventVariant)
        );
    }
}

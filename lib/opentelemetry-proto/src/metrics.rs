use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{TimeZone, Utc};
use lookup::path;
use vector_core::event::{
    Event, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
    metric::{Bucket, Quantile, TagValue},
};
use vrl::value::{ObjectMap, Value};

use super::proto::{
    common::v1::{InstrumentationScope, KeyValue, any_value::Value as PBValue},
    metrics::v1::{
        AggregationTemporality, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge,
        Histogram, HistogramDataPoint, NumberDataPoint, ResourceMetrics, Sum, Summary,
        SummaryDataPoint, metric::Data, number_data_point::Value as NumberDataPointValue,
    },
    resource::v1::Resource,
};

/// Shared context for converting OTLP metric data points into Vector events.
/// Groups the fields that are identical across all data points within a single
/// OTLP Metric message, avoiding repetitive argument passing.
struct MetricContext {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    metric_name: String,
    scope_schema_url: String,
    resource_schema_url: String,
}

impl ResourceMetrics {
    pub fn into_event_iter(self) -> impl Iterator<Item = Event> {
        let resource = self.resource;
        let resource_schema_url = self.schema_url;

        self.scope_metrics
            .into_iter()
            .flat_map(move |scope_metrics| {
                let scope = scope_metrics.scope;
                let scope_schema_url = scope_metrics.schema_url;
                let resource = resource.clone();
                let resource_schema_url = resource_schema_url.clone();

                scope_metrics.metrics.into_iter().flat_map(move |metric| {
                    let ctx = MetricContext {
                        resource: resource.clone(),
                        scope: scope.clone(),
                        metric_name: metric.name,
                        scope_schema_url: scope_schema_url.clone(),
                        resource_schema_url: resource_schema_url.clone(),
                    };
                    match metric.data {
                        Some(Data::Gauge(g)) => Self::convert_gauge(g, ctx),
                        Some(Data::Sum(s)) => Self::convert_sum(s, ctx),
                        Some(Data::Histogram(h)) => Self::convert_histogram(h, ctx),
                        Some(Data::ExponentialHistogram(e)) => {
                            Self::convert_exp_histogram(e, ctx)
                        }
                        Some(Data::Summary(su)) => Self::convert_summary(su, ctx),
                        _ => Vec::new(),
                    }
                })
            })
    }

    fn convert_gauge(gauge: Gauge, ctx: MetricContext) -> Vec<Event> {
        gauge
            .data_points
            .into_iter()
            .map(move |point| {
                GaugeMetric {
                    resource: ctx.resource.clone(),
                    scope: ctx.scope.clone(),
                    point,
                }
                .into_metric(
                    ctx.metric_name.clone(),
                    &ctx.scope_schema_url,
                    &ctx.resource_schema_url,
                )
            })
            .collect()
    }

    fn convert_sum(sum: Sum, ctx: MetricContext) -> Vec<Event> {
        sum.data_points
            .into_iter()
            .map(move |point| {
                SumMetric {
                    aggregation_temporality: sum.aggregation_temporality,
                    resource: ctx.resource.clone(),
                    scope: ctx.scope.clone(),
                    is_monotonic: sum.is_monotonic,
                    point,
                }
                .into_metric(
                    ctx.metric_name.clone(),
                    &ctx.scope_schema_url,
                    &ctx.resource_schema_url,
                )
            })
            .collect()
    }

    fn convert_histogram(histogram: Histogram, ctx: MetricContext) -> Vec<Event> {
        histogram
            .data_points
            .into_iter()
            .map(move |point| {
                HistogramMetric {
                    aggregation_temporality: histogram.aggregation_temporality,
                    resource: ctx.resource.clone(),
                    scope: ctx.scope.clone(),
                    point,
                }
                .into_metric(
                    ctx.metric_name.clone(),
                    &ctx.scope_schema_url,
                    &ctx.resource_schema_url,
                )
            })
            .collect()
    }

    fn convert_exp_histogram(histogram: ExponentialHistogram, ctx: MetricContext) -> Vec<Event> {
        histogram
            .data_points
            .into_iter()
            .map(move |point| {
                ExpHistogramMetric {
                    aggregation_temporality: histogram.aggregation_temporality,
                    resource: ctx.resource.clone(),
                    scope: ctx.scope.clone(),
                    point,
                }
                .into_metric(
                    ctx.metric_name.clone(),
                    &ctx.scope_schema_url,
                    &ctx.resource_schema_url,
                )
            })
            .collect()
    }

    fn convert_summary(summary: Summary, ctx: MetricContext) -> Vec<Event> {
        summary
            .data_points
            .into_iter()
            .map(move |point| {
                SummaryMetric {
                    resource: ctx.resource.clone(),
                    scope: ctx.scope.clone(),
                    point,
                }
                .into_metric(
                    ctx.metric_name.clone(),
                    &ctx.scope_schema_url,
                    &ctx.resource_schema_url,
                )
            })
            .collect()
    }
}

struct GaugeMetric {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    point: NumberDataPoint,
}

struct SumMetric {
    aggregation_temporality: i32,
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    point: NumberDataPoint,
    is_monotonic: bool,
}

struct SummaryMetric {
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    point: SummaryDataPoint,
}

struct HistogramMetric {
    aggregation_temporality: i32,
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    point: HistogramDataPoint,
}

struct ExpHistogramMetric {
    aggregation_temporality: i32,
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    point: ExponentialHistogramDataPoint,
}

pub fn build_metric_tags(
    resource: Option<Resource>,
    scope: Option<InstrumentationScope>,
    attributes: &[KeyValue],
    scope_schema_url: &str,
    resource_schema_url: &str,
) -> MetricTags {
    let mut tags = MetricTags::default();

    if let Some(res) = resource {
        for attr in res.attributes {
            if let Some(value) = &attr.value
                && let Some(pb_value) = &value.value
            {
                tags.insert(
                    format!("resource.{}", attr.key),
                    TagValue::from(pb_value.clone()),
                );
            }
        }
        if res.dropped_attributes_count > 0 {
            tags.insert(
                "resource_dropped_attributes_count".to_string(),
                res.dropped_attributes_count.to_string(),
            );
        }
    }

    if let Some(scope) = scope {
        if !scope.name.is_empty() {
            tags.insert("scope.name".to_string(), scope.name);
        }
        if !scope.version.is_empty() {
            tags.insert("scope.version".to_string(), scope.version);
        }
        if scope.dropped_attributes_count > 0 {
            tags.insert(
                "scope_dropped_attributes_count".to_string(),
                scope.dropped_attributes_count.to_string(),
            );
        }
        for attr in scope.attributes {
            if let Some(value) = &attr.value
                && let Some(pb_value) = &value.value
            {
                tags.insert(
                    format!("scope.{}", attr.key),
                    TagValue::from(pb_value.clone()),
                );
            }
        }
    }

    // Use underscore-separated names for metadata tags to avoid colliding
    // with user-supplied resource/scope attributes that use dot-separated
    // "resource.{key}" / "scope.{key}" format.
    if !scope_schema_url.is_empty() {
        tags.insert(
            "scope_schema_url".to_string(),
            scope_schema_url.to_string(),
        );
    }
    if !resource_schema_url.is_empty() {
        tags.insert(
            "resource_schema_url".to_string(),
            resource_schema_url.to_string(),
        );
    }

    for attr in attributes {
        if let Some(value) = &attr.value
            && let Some(pb_value) = &value.value
        {
            tags.insert(attr.key.clone(), TagValue::from(pb_value.clone()));
        }
    }

    tags
}

/// Stash `start_time_unix_nano` in metric metadata for OTLP roundtrip preservation.
///
/// Stored at `%vector.otlp.start_time_unix_nano` in EventMetadata.
/// Only stored when non-zero (zero means "not set" in OTLP).
/// The OTLP encoder reads this back to avoid losing start_time on roundtrip.
fn stash_start_time(metric: &mut MetricEvent, start_time_unix_nano: u64) {
    if start_time_unix_nano > 0 {
        metric.metadata_mut().value_mut().insert(
            path!("vector", "otlp", "start_time_unix_nano"),
            Value::Integer(start_time_unix_nano as i64),
        );
    }
}

/// Compute a fingerprint of the metric's tags for sidecar staleness detection.
///
/// The fingerprint is a hash of the sorted (key, value) pairs from `MetricTags`.
/// On the encode side, the same fingerprint is recomputed and compared to the
/// sidecar's stored value. If they match, the sidecar's typed attributes are
/// still valid and can be used instead of the stringified tag decomposition.
fn compute_tags_fingerprint(tags: &MetricTags) -> i64 {
    let mut hasher = DefaultHasher::new();
    for (key, value) in tags.iter_single() {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    hasher.finish() as i64
}

/// Convert a protobuf AnyValue variant to a VRL Value wrapped in a single-key
/// Object named after the OTLP kind. This preserves the distinction between
/// StringValue/BytesValue (both map to Value::Bytes) and between ArrayValue/
/// KvlistValue (both map to Value::Array/Object), so the encoder can reconstruct
/// the exact PBValue variant on output.
///
/// Examples:
///   StringValue("x")  → {"string_value": "x"}
///   BytesValue([1,2]) → {"bytes_value": [1,2]}
///   IntValue(42)      → {"int_value": 42}
///   ArrayValue([...]) → {"array_value": [...wrapped values...]}
fn pb_value_to_typed_value(pb: PBValue) -> Value {
    let (key, val) = match pb {
        PBValue::StringValue(s) => ("string_value", Value::from(s)),
        PBValue::BoolValue(b) => ("bool_value", Value::Boolean(b)),
        PBValue::IntValue(i) => ("int_value", Value::Integer(i)),
        PBValue::DoubleValue(f) => (
            "double_value",
            ordered_float::NotNan::new(f)
                .map(Value::Float)
                .unwrap_or(Value::Null),
        ),
        PBValue::BytesValue(b) => ("bytes_value", Value::Bytes(bytes::Bytes::from(b))),
        PBValue::ArrayValue(arr) => (
            "array_value",
            Value::Array(
                arr.values
                    .into_iter()
                    .map(|av| av.value.map(pb_value_to_typed_value).unwrap_or(Value::Null))
                    .collect(),
            ),
        ),
        PBValue::KvlistValue(kvl) => ("kvlist_value", kv_list_into_typed_value(kvl.values)),
    };
    Value::Object(ObjectMap::from([(key.into(), val)]))
}

/// Convert a Vec<KeyValue> into a VRL Object where each value is wrapped
/// by its OTLP kind via `pb_value_to_typed_value`.
fn kv_list_into_typed_value(attrs: Vec<KeyValue>) -> Value {
    Value::Object(
        attrs
            .into_iter()
            .filter_map(|kv| {
                kv.value.and_then(|av| {
                    av.value
                        .map(|v| (kv.key.into(), pb_value_to_typed_value(v)))
                })
            })
            .collect::<ObjectMap>(),
    )
}

/// Build a sidecar VRL object containing the original typed OTLP attributes.
///
/// Each attribute value is wrapped by its OTLP kind (e.g. IntValue(42) becomes
/// {"int_value": 42}) so the encoder can reconstruct the exact protobuf variant.
/// This preserves StringValue/BytesValue distinction and handles ArrayValue/KvlistValue.
///
/// Takes references to avoid consuming the resource/scope before `build_metric_tags()`.
fn build_otlp_sidecar_data(
    resource: Option<&Resource>,
    scope: Option<&InstrumentationScope>,
    data_point_attributes: &[KeyValue],
) -> ObjectMap {
    let mut sidecar = ObjectMap::new();

    if let Some(res) = resource {
        if !res.attributes.is_empty() {
            sidecar.insert(
                "resource_attributes".into(),
                kv_list_into_typed_value(res.attributes.clone()),
            );
        }
        if res.dropped_attributes_count > 0 {
            sidecar.insert(
                "resource_dropped_attributes_count".into(),
                Value::Integer(i64::from(res.dropped_attributes_count)),
            );
        }
    }

    if let Some(scope) = scope {
        if !scope.attributes.is_empty() {
            sidecar.insert(
                "scope_attributes".into(),
                kv_list_into_typed_value(scope.attributes.clone()),
            );
        }
        if !scope.name.is_empty() {
            sidecar.insert("scope_name".into(), Value::from(scope.name.clone()));
        }
        if !scope.version.is_empty() {
            sidecar.insert("scope_version".into(), Value::from(scope.version.clone()));
        }
        if scope.dropped_attributes_count > 0 {
            sidecar.insert(
                "scope_dropped_attributes_count".into(),
                Value::Integer(i64::from(scope.dropped_attributes_count)),
            );
        }
    }

    if !data_point_attributes.is_empty() {
        sidecar.insert(
            "data_point_attributes".into(),
            kv_list_into_typed_value(data_point_attributes.to_vec()),
        );
    }

    sidecar
}

/// Stash the typed OTLP sidecar in metric metadata at `%vector.otlp.metric_sidecar`.
///
/// Computes a fingerprint from the metric's current tags and embeds it in the
/// sidecar for staleness detection on the encode side.
fn stash_otlp_sidecar(metric: &mut MetricEvent, mut sidecar: ObjectMap) {
    if sidecar.is_empty() {
        return;
    }

    if let Some(tags) = metric.tags() {
        sidecar.insert(
            "tags_fingerprint".into(),
            Value::Integer(compute_tags_fingerprint(tags)),
        );
    }

    metric.metadata_mut().value_mut().insert(
        path!("vector", "otlp", "metric_sidecar"),
        Value::Object(sidecar),
    );
}

impl SumMetric {
    fn into_metric(
        self,
        metric_name: String,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> Event {
        let timestamp = if self.point.time_unix_nano == 0 {
            None
        } else {
            Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64))
        };
        let value = self.point.value.to_f64().unwrap_or(0.0);
        let sidecar = build_otlp_sidecar_data(
            self.resource.as_ref(),
            self.scope.as_ref(),
            &self.point.attributes,
        );
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );
        let kind = if self.aggregation_temporality == AggregationTemporality::Delta as i32 {
            MetricKind::Incremental
        } else {
            MetricKind::Absolute
        };

        // as per otel doc non_monotonic sum would be better transformed to gauge in time-series
        let metric_value = if self.is_monotonic {
            MetricValue::Counter { value }
        } else {
            MetricValue::Gauge { value }
        };

        let mut metric = MetricEvent::new(metric_name, kind, metric_value)
            .with_tags(Some(attributes))
            .with_timestamp(timestamp);
        stash_start_time(&mut metric, self.point.start_time_unix_nano);
        stash_otlp_sidecar(&mut metric, sidecar);
        metric.into()
    }
}

impl GaugeMetric {
    fn into_metric(
        self,
        metric_name: String,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> Event {
        let timestamp = if self.point.time_unix_nano == 0 {
            None
        } else {
            Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64))
        };
        let value = self.point.value.to_f64().unwrap_or(0.0);
        let sidecar = build_otlp_sidecar_data(
            self.resource.as_ref(),
            self.scope.as_ref(),
            &self.point.attributes,
        );
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );

        let mut metric = MetricEvent::new(
            metric_name,
            MetricKind::Absolute,
            MetricValue::Gauge { value },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes));
        stash_start_time(&mut metric, self.point.start_time_unix_nano);
        stash_otlp_sidecar(&mut metric, sidecar);
        metric.into()
    }
}

impl HistogramMetric {
    fn into_metric(
        self,
        metric_name: String,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> Event {
        let timestamp = if self.point.time_unix_nano == 0 {
            None
        } else {
            Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64))
        };
        let sidecar = build_otlp_sidecar_data(
            self.resource.as_ref(),
            self.scope.as_ref(),
            &self.point.attributes,
        );
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );
        let buckets = match self.point.bucket_counts.len() {
            0 => Vec::new(),
            n => {
                let mut buckets = Vec::with_capacity(n);

                for (i, &count) in self.point.bucket_counts.iter().enumerate() {
                    // there are n+1 buckets, since we have -Inf, +Inf on the sides
                    let upper_limit = self
                        .point
                        .explicit_bounds
                        .get(i)
                        .copied()
                        .unwrap_or(f64::INFINITY);
                    buckets.push(Bucket { count, upper_limit });
                }

                buckets
            }
        };

        let kind = if self.aggregation_temporality == AggregationTemporality::Delta as i32 {
            MetricKind::Incremental
        } else {
            MetricKind::Absolute
        };

        let mut metric = MetricEvent::new(
            metric_name,
            kind,
            MetricValue::AggregatedHistogram {
                buckets,
                count: self.point.count,
                sum: self.point.sum.unwrap_or(0.0),
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes));
        stash_start_time(&mut metric, self.point.start_time_unix_nano);
        stash_otlp_sidecar(&mut metric, sidecar);
        metric.into()
    }
}

impl ExpHistogramMetric {
    fn into_metric(
        self,
        metric_name: String,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> Event {
        // we have to convert Exponential Histogram to agg histogram using scale and base
        let timestamp = if self.point.time_unix_nano == 0 {
            None
        } else {
            Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64))
        };
        let sidecar = build_otlp_sidecar_data(
            self.resource.as_ref(),
            self.scope.as_ref(),
            &self.point.attributes,
        );
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );

        let scale = self.point.scale;
        // from Opentelemetry docs: base = 2**(2**(-scale))
        let base = 2f64.powf(2f64.powi(-scale));

        let mut buckets = Vec::new();

        if let Some(negative_buckets) = self.point.negative {
            for (i, &count) in negative_buckets.bucket_counts.iter().enumerate() {
                let index = negative_buckets.offset + i as i32;
                let upper_limit = -base.powi(index);
                buckets.push(Bucket { count, upper_limit });
            }
        }

        if self.point.zero_count > 0 {
            buckets.push(Bucket {
                count: self.point.zero_count,
                upper_limit: 0.0,
            });
        }

        if let Some(positive_buckets) = self.point.positive {
            for (i, &count) in positive_buckets.bucket_counts.iter().enumerate() {
                let index = positive_buckets.offset + i as i32;
                let upper_limit = base.powi(index + 1);
                buckets.push(Bucket { count, upper_limit });
            }
        }

        let kind = if self.aggregation_temporality == AggregationTemporality::Delta as i32 {
            MetricKind::Incremental
        } else {
            MetricKind::Absolute
        };

        let mut metric = MetricEvent::new(
            metric_name,
            kind,
            MetricValue::AggregatedHistogram {
                buckets,
                count: self.point.count,
                sum: self.point.sum.unwrap_or(0.0),
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes));
        stash_start_time(&mut metric, self.point.start_time_unix_nano);
        stash_otlp_sidecar(&mut metric, sidecar);
        metric.into()
    }
}

impl SummaryMetric {
    fn into_metric(
        self,
        metric_name: String,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> Event {
        let timestamp = if self.point.time_unix_nano == 0 {
            None
        } else {
            Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64))
        };
        let sidecar = build_otlp_sidecar_data(
            self.resource.as_ref(),
            self.scope.as_ref(),
            &self.point.attributes,
        );
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );

        let quantiles: Vec<Quantile> = self
            .point
            .quantile_values
            .iter()
            .map(|q| Quantile {
                quantile: q.quantile,
                value: q.value,
            })
            .collect();

        let mut metric = MetricEvent::new(
            metric_name,
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles,
                count: self.point.count,
                sum: self.point.sum,
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes));
        stash_start_time(&mut metric, self.point.start_time_unix_nano);
        stash_otlp_sidecar(&mut metric, sidecar);
        metric.into()
    }
}

pub trait ToF64 {
    fn to_f64(self) -> Option<f64>;
}

impl ToF64 for Option<NumberDataPointValue> {
    fn to_f64(self) -> Option<f64> {
        match self {
            Some(NumberDataPointValue::AsDouble(f)) => Some(f),
            Some(NumberDataPointValue::AsInt(i)) => Some(i as f64),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{
        common::v1::{AnyValue, KeyValue, any_value::Value as PBValue},
        metrics::v1::{Gauge, Metric, ScopeMetrics},
    };

    fn make_kv(key: &str, val: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::StringValue(val.to_string())),
            }),
        }
    }

    fn make_resource_metrics(
        resource_attrs: Vec<KeyValue>,
        resource_dropped: u32,
        scope: Option<InstrumentationScope>,
        scope_schema_url: &str,
        resource_schema_url: &str,
    ) -> ResourceMetrics {
        ResourceMetrics {
            resource: Some(Resource {
                attributes: resource_attrs,
                dropped_attributes_count: resource_dropped,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope,
                metrics: vec![Metric {
                    name: "test_gauge".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![make_kv("env", "prod")],
                            start_time_unix_nano: 0,
                            time_unix_nano: 1_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsDouble(42.0)),
                        }],
                    })),
                }],
                schema_url: scope_schema_url.to_string(),
            }],
            schema_url: resource_schema_url.to_string(),
        }
    }

    fn get_tags(event: &Event) -> &MetricTags {
        event.as_metric().tags().unwrap()
    }

    //
    // Tests for schema_url tags
    //

    #[test]
    fn test_scope_schema_url_as_tag() {
        let rm = make_resource_metrics(vec![], 0, None, "https://scope.schema", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        let tags = get_tags(&events[0]);
        assert_eq!(
            tags.get("scope_schema_url").unwrap(),
            "https://scope.schema"
        );
    }

    #[test]
    fn test_resource_schema_url_as_tag() {
        let rm = make_resource_metrics(vec![], 0, None, "", "https://resource.schema");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert_eq!(
            tags.get("resource_schema_url").unwrap(),
            "https://resource.schema"
        );
    }

    #[test]
    fn test_empty_schema_urls_not_tagged() {
        let rm = make_resource_metrics(vec![], 0, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert!(tags.get("scope_schema_url").is_none());
        assert!(tags.get("resource_schema_url").is_none());
    }

    //
    // Tests for dropped_attributes_count tags
    //

    #[test]
    fn test_scope_dropped_attributes_count_tag() {
        let scope = InstrumentationScope {
            name: "tracer".to_string(),
            version: String::new(),
            attributes: vec![],
            dropped_attributes_count: 5,
        };
        let rm = make_resource_metrics(vec![], 0, Some(scope), "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert_eq!(tags.get("scope_dropped_attributes_count").unwrap(), "5");
    }

    #[test]
    fn test_scope_dropped_zero_not_tagged() {
        let scope = InstrumentationScope {
            name: "tracer".to_string(),
            version: String::new(),
            attributes: vec![],
            dropped_attributes_count: 0,
        };
        let rm = make_resource_metrics(vec![], 0, Some(scope), "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert!(tags.get("scope_dropped_attributes_count").is_none());
    }

    #[test]
    fn test_resource_dropped_attributes_count_tag() {
        let rm = make_resource_metrics(vec![make_kv("host", "server")], 3, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert_eq!(tags.get("resource_dropped_attributes_count").unwrap(), "3");
    }

    #[test]
    fn test_resource_dropped_zero_not_tagged() {
        let rm = make_resource_metrics(vec![make_kv("host", "server")], 0, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert!(tags.get("resource_dropped_attributes_count").is_none());
    }

    //
    // Combined: all new tags
    //

    //
    // Tests for start_time_unix_nano preservation
    //

    fn get_start_time(event: &Event) -> Option<i64> {
        event
            .as_metric()
            .metadata()
            .value()
            .get("vector")
            .and_then(|v| v.get("otlp"))
            .and_then(|v| v.get("start_time_unix_nano"))
            .and_then(|v| match v {
                Value::Integer(i) => Some(*i),
                _ => None,
            })
    }

    #[test]
    fn test_start_time_preserved_for_gauge() {
        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_gauge".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 1_000_000_000,
                            time_unix_nano: 2_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsDouble(1.0)),
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(get_start_time(&events[0]), Some(1_000_000_000));
    }

    #[test]
    fn test_start_time_zero_not_stored() {
        // start_time_unix_nano=0 means "not set" in OTLP, should not be stored
        let rm = make_resource_metrics(vec![], 0, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(get_start_time(&events[0]), None);
    }

    #[test]
    fn test_start_time_preserved_for_sum() {
        use crate::proto::metrics::v1::Sum;

        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_sum".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Sum(Sum {
                        aggregation_temporality: AggregationTemporality::Cumulative as i32,
                        is_monotonic: true,
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 5_000_000_000,
                            time_unix_nano: 10_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsInt(100)),
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(get_start_time(&events[0]), Some(5_000_000_000));
    }

    #[test]
    fn test_start_time_preserved_for_histogram() {
        use crate::proto::metrics::v1::Histogram;

        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_histogram".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Histogram(Histogram {
                        aggregation_temporality: AggregationTemporality::Delta as i32,
                        data_points: vec![HistogramDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 3_000_000_000,
                            time_unix_nano: 4_000_000_000,
                            count: 10,
                            sum: Some(100.0),
                            bucket_counts: vec![2, 5, 3],
                            explicit_bounds: vec![10.0, 50.0],
                            exemplars: vec![],
                            flags: 0,
                            min: None,
                            max: None,
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(get_start_time(&events[0]), Some(3_000_000_000));
    }

    #[test]
    fn test_start_time_preserved_for_summary() {
        use crate::proto::metrics::v1::Summary;

        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_summary".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Summary(Summary {
                        data_points: vec![SummaryDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 7_000_000_000,
                            time_unix_nano: 8_000_000_000,
                            count: 5,
                            sum: 50.0,
                            quantile_values: vec![],
                            flags: 0,
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(get_start_time(&events[0]), Some(7_000_000_000));
    }

    //
    // Combined: all new tags
    //

    #[test]
    fn test_all_new_tags_together() {
        let scope = InstrumentationScope {
            name: "metrics-sdk".to_string(),
            version: "2.0.0".to_string(),
            attributes: vec![make_kv("lib.lang", "go")],
            dropped_attributes_count: 1,
        };
        let rm = make_resource_metrics(
            vec![make_kv("service.name", "api")],
            2,
            Some(scope),
            "https://scope.schema",
            "https://resource.schema",
        );
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);

        // Existing tags still work
        assert_eq!(tags.get("scope.name").unwrap(), "metrics-sdk");
        assert_eq!(tags.get("scope.version").unwrap(), "2.0.0");
        assert_eq!(tags.get("resource.service.name").unwrap(), "api");
        assert_eq!(tags.get("env").unwrap(), "prod");

        // New tags
        assert_eq!(
            tags.get("scope_schema_url").unwrap(),
            "https://scope.schema"
        );
        assert_eq!(
            tags.get("resource_schema_url").unwrap(),
            "https://resource.schema"
        );
        assert_eq!(tags.get("scope_dropped_attributes_count").unwrap(), "1");
        assert_eq!(tags.get("resource_dropped_attributes_count").unwrap(), "2");
    }

    //
    // Tests for typed metric sidecar
    //

    fn make_int_kv(key: &str, val: i64) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::IntValue(val)),
            }),
        }
    }

    fn make_bool_kv(key: &str, val: bool) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::BoolValue(val)),
            }),
        }
    }

    fn make_double_kv(key: &str, val: f64) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(PBValue::DoubleValue(val)),
            }),
        }
    }

    fn get_sidecar(event: &Event) -> Option<&ObjectMap> {
        event
            .as_metric()
            .metadata()
            .value()
            .get("vector")
            .and_then(|v| v.get("otlp"))
            .and_then(|v| v.get("metric_sidecar"))
            .and_then(|v| v.as_object())
    }

    /// Extract the inner value from a typed-wrapper sidecar value.
    /// e.g. {"int_value": 42} → ("int_value", &Value::Integer(42))
    fn unwrap_typed(v: &Value) -> (&str, &Value) {
        let obj = v.as_object().expect("typed value should be an Object");
        assert_eq!(obj.len(), 1, "typed wrapper should have exactly one key");
        let (k, v) = obj.iter().next().unwrap();
        (k.as_str(), v)
    }

    #[test]
    fn test_sidecar_preserves_typed_resource_attributes() {
        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    make_int_kv("http.status_code", 200),
                    make_bool_kv("http.ok", true),
                    make_double_kv("load.avg", 0.75),
                    make_kv("service.name", "web"),
                ],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_gauge".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 0,
                            time_unix_nano: 1_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsDouble(1.0)),
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        let sidecar = get_sidecar(&events[0]).expect("sidecar should be present");

        let res_attrs = sidecar
            .get("resource_attributes")
            .and_then(|v| v.as_object())
            .expect("resource_attributes should be an object");

        // Each value is wrapped by its OTLP kind
        let (kind, val) = unwrap_typed(res_attrs.get("http.status_code").unwrap());
        assert_eq!(kind, "int_value");
        assert_eq!(val, &Value::Integer(200));

        let (kind, val) = unwrap_typed(res_attrs.get("http.ok").unwrap());
        assert_eq!(kind, "bool_value");
        assert_eq!(val, &Value::Boolean(true));

        let (kind, val) = unwrap_typed(res_attrs.get("load.avg").unwrap());
        assert_eq!(kind, "double_value");
        match val {
            Value::Float(f) => assert!((f.into_inner() - 0.75).abs() < f64::EPSILON),
            other => panic!("Expected Float, got {other:?}"),
        }

        let (kind, val) = unwrap_typed(res_attrs.get("service.name").unwrap());
        assert_eq!(kind, "string_value");
        assert_eq!(val, &Value::from("web".to_string()));
    }

    #[test]
    fn test_sidecar_preserves_typed_data_point_attributes() {
        let rm = ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_gauge".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![
                                make_int_kv("count", 42),
                                make_bool_kv("is_error", false),
                            ],
                            start_time_unix_nano: 0,
                            time_unix_nano: 1_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsDouble(1.0)),
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        let sidecar = get_sidecar(&events[0]).expect("sidecar should be present");

        let dp_attrs = sidecar
            .get("data_point_attributes")
            .and_then(|v| v.as_object())
            .expect("data_point_attributes should be an object");

        let (kind, val) = unwrap_typed(dp_attrs.get("count").unwrap());
        assert_eq!(kind, "int_value");
        assert_eq!(val, &Value::Integer(42));

        let (kind, val) = unwrap_typed(dp_attrs.get("is_error").unwrap());
        assert_eq!(kind, "bool_value");
        assert_eq!(val, &Value::Boolean(false));
    }

    #[test]
    fn test_sidecar_includes_scope_metadata() {
        let scope = InstrumentationScope {
            name: "my-meter".to_string(),
            version: "1.2.3".to_string(),
            attributes: vec![make_bool_kv("lib.debug", true)],
            dropped_attributes_count: 7,
        };
        let rm = make_resource_metrics(vec![], 0, Some(scope), "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let sidecar = get_sidecar(&events[0]).expect("sidecar should be present");

        assert_eq!(
            sidecar.get("scope_name").unwrap(),
            &Value::from("my-meter".to_string())
        );
        assert_eq!(
            sidecar.get("scope_version").unwrap(),
            &Value::from("1.2.3".to_string())
        );
        assert_eq!(
            sidecar.get("scope_dropped_attributes_count").unwrap(),
            &Value::Integer(7)
        );

        let scope_attrs = sidecar
            .get("scope_attributes")
            .and_then(|v| v.as_object())
            .expect("scope_attributes should be an object");
        let (kind, val) = unwrap_typed(scope_attrs.get("lib.debug").unwrap());
        assert_eq!(kind, "bool_value");
        assert_eq!(val, &Value::Boolean(true));
    }

    #[test]
    fn test_sidecar_contains_tags_fingerprint() {
        let rm = make_resource_metrics(
            vec![make_kv("service.name", "api")],
            0,
            None,
            "",
            "",
        );
        let events: Vec<Event> = rm.into_event_iter().collect();
        let sidecar = get_sidecar(&events[0]).expect("sidecar should be present");

        // Fingerprint is present
        assert!(sidecar.get("tags_fingerprint").is_some());

        // Fingerprint matches recomputed value from tags
        let stored_fp = match sidecar.get("tags_fingerprint").unwrap() {
            Value::Integer(i) => *i,
            other => panic!("Expected Integer fingerprint, got {other:?}"),
        };
        let tags = get_tags(&events[0]);
        let recomputed_fp = compute_tags_fingerprint(tags);
        assert_eq!(stored_fp, recomputed_fp);
    }

    #[test]
    fn test_sidecar_not_present_when_no_attributes() {
        // No resource attrs, no scope, no data point attrs → empty sidecar → not stored
        let rm = ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "test_gauge".to_string(),
                    description: String::new(),
                    unit: String::new(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 0,
                            time_unix_nano: 1_000_000_000,
                            exemplars: vec![],
                            flags: 0,
                            value: Some(NumberDataPointValue::AsDouble(1.0)),
                        }],
                    })),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        };
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert!(get_sidecar(&events[0]).is_none());
    }
}

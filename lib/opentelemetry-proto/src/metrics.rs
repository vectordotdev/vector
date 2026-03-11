use chrono::{TimeZone, Utc};
use tracing::warn;
use vector_core::event::{
    Event, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
    metric::{Bucket, Quantile, TagValue},
};

use super::proto::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue, any_value::Value as PBValue},
    metrics::v1::{
        AggregationTemporality, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge,
        Histogram, HistogramDataPoint, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
        Summary, SummaryDataPoint, metric::Data, number_data_point::Value as NumberDataPointValue,
        summary_data_point::ValueAtQuantile,
    },
    resource::v1::Resource,
};

impl ResourceMetrics {
    pub fn into_event_iter(self) -> impl Iterator<Item = Event> {
        let resource = self.resource.clone();

        self.scope_metrics
            .into_iter()
            .flat_map(move |scope_metrics| {
                let scope = scope_metrics.scope;
                let resource = resource.clone();

                scope_metrics.metrics.into_iter().flat_map(move |metric| {
                    let metric_name = metric.name.clone();
                    match metric.data {
                        Some(Data::Gauge(g)) => {
                            Self::convert_gauge(g, &resource, &scope, &metric_name)
                        }
                        Some(Data::Sum(s)) => Self::convert_sum(s, &resource, &scope, &metric_name),
                        Some(Data::Histogram(h)) => {
                            Self::convert_histogram(h, &resource, &scope, &metric_name)
                        }
                        Some(Data::ExponentialHistogram(e)) => {
                            Self::convert_exp_histogram(e, &resource, &scope, &metric_name)
                        }
                        Some(Data::Summary(su)) => {
                            Self::convert_summary(su, &resource, &scope, &metric_name)
                        }
                        _ => Vec::new(),
                    }
                })
            })
    }

    fn convert_gauge(
        gauge: Gauge,
        resource: &Option<Resource>,
        scope: &Option<InstrumentationScope>,
        metric_name: &str,
    ) -> Vec<Event> {
        let resource = resource.clone();
        let scope = scope.clone();
        let metric_name = metric_name.to_string();

        gauge
            .data_points
            .into_iter()
            .map(move |point| {
                GaugeMetric {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    point,
                }
                .into_metric(metric_name.clone())
            })
            .collect()
    }

    fn convert_sum(
        sum: Sum,
        resource: &Option<Resource>,
        scope: &Option<InstrumentationScope>,
        metric_name: &str,
    ) -> Vec<Event> {
        let resource = resource.clone();
        let scope = scope.clone();
        let metric_name = metric_name.to_string();

        sum.data_points
            .into_iter()
            .map(move |point| {
                SumMetric {
                    aggregation_temporality: sum.aggregation_temporality,
                    resource: resource.clone(),
                    scope: scope.clone(),
                    is_monotonic: sum.is_monotonic,
                    point,
                }
                .into_metric(metric_name.clone())
            })
            .collect()
    }

    fn convert_histogram(
        histogram: Histogram,
        resource: &Option<Resource>,
        scope: &Option<InstrumentationScope>,
        metric_name: &str,
    ) -> Vec<Event> {
        let resource = resource.clone();
        let scope = scope.clone();
        let metric_name = metric_name.to_string();

        histogram
            .data_points
            .into_iter()
            .map(move |point| {
                HistogramMetric {
                    aggregation_temporality: histogram.aggregation_temporality,
                    resource: resource.clone(),
                    scope: scope.clone(),
                    point,
                }
                .into_metric(metric_name.clone())
            })
            .collect()
    }

    fn convert_exp_histogram(
        histogram: ExponentialHistogram,
        resource: &Option<Resource>,
        scope: &Option<InstrumentationScope>,
        metric_name: &str,
    ) -> Vec<Event> {
        let resource = resource.clone();
        let scope = scope.clone();
        let metric_name = metric_name.to_string();

        histogram
            .data_points
            .into_iter()
            .map(move |point| {
                ExpHistogramMetric {
                    aggregation_temporality: histogram.aggregation_temporality,
                    resource: resource.clone(),
                    scope: scope.clone(),
                    point,
                }
                .into_metric(metric_name.clone())
            })
            .collect()
    }

    fn convert_summary(
        summary: Summary,
        resource: &Option<Resource>,
        scope: &Option<InstrumentationScope>,
        metric_name: &str,
    ) -> Vec<Event> {
        let resource = resource.clone();
        let scope = scope.clone();
        let metric_name = metric_name.to_string();

        summary
            .data_points
            .into_iter()
            .map(move |point| {
                SummaryMetric {
                    resource: resource.clone(),
                    scope: scope.clone(),
                    point,
                }
                .into_metric(metric_name.clone())
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
) -> MetricTags {
    let mut tags = MetricTags::default();

    if let Some(res) = resource {
        for attr in res.attributes {
            if let Some(value) = &attr.value
                && let Some(pb_value) = &value.value
            {
                tags.insert(
                    format!("resource.{}", attr.key.clone()),
                    TagValue::from(pb_value.clone()),
                );
            }
        }
    }

    if let Some(scope) = scope {
        if !scope.name.is_empty() {
            tags.insert("scope.name".to_string(), scope.name);
        }
        if !scope.version.is_empty() {
            tags.insert("scope.version".to_string(), scope.version);
        }
        for attr in scope.attributes {
            if let Some(value) = &attr.value
                && let Some(pb_value) = &value.value
            {
                tags.insert(
                    format!("scope.{}", attr.key.clone()),
                    TagValue::from(pb_value.clone()),
                );
            }
        }
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

impl SumMetric {
    fn into_metric(self, metric_name: String) -> Event {
        let timestamp = Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64));
        let value = self.point.value.to_f64().unwrap_or(0.0);
        let attributes = build_metric_tags(self.resource, self.scope, &self.point.attributes);
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

        MetricEvent::new(metric_name, kind, metric_value)
            .with_tags(Some(attributes))
            .with_timestamp(timestamp)
            .into()
    }
}

impl GaugeMetric {
    fn into_metric(self, metric_name: String) -> Event {
        let timestamp = Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64));
        let value = self.point.value.to_f64().unwrap_or(0.0);
        let attributes = build_metric_tags(self.resource, self.scope, &self.point.attributes);

        MetricEvent::new(
            metric_name,
            MetricKind::Absolute,
            MetricValue::Gauge { value },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes))
        .into()
    }
}

impl HistogramMetric {
    fn into_metric(self, metric_name: String) -> Event {
        let timestamp = Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64));
        let attributes = build_metric_tags(self.resource, self.scope, &self.point.attributes);
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

        MetricEvent::new(
            metric_name,
            kind,
            MetricValue::AggregatedHistogram {
                buckets,
                count: self.point.count,
                sum: self.point.sum.unwrap_or(0.0),
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes))
        .into()
    }
}

impl ExpHistogramMetric {
    fn into_metric(self, metric_name: String) -> Event {
        // we have to convert Exponential Histogram to agg histogram using scale and base
        let timestamp = Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64));
        let attributes = build_metric_tags(self.resource, self.scope, &self.point.attributes);

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

        MetricEvent::new(
            metric_name,
            kind,
            MetricValue::AggregatedHistogram {
                buckets,
                count: self.point.count,
                sum: self.point.sum.unwrap_or(0.0),
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes))
        .into()
    }
}

impl SummaryMetric {
    fn into_metric(self, metric_name: String) -> Event {
        let timestamp = Some(Utc.timestamp_nanos(self.point.time_unix_nano as i64));
        let attributes = build_metric_tags(self.resource, self.scope, &self.point.attributes);

        let quantiles: Vec<Quantile> = self
            .point
            .quantile_values
            .iter()
            .map(|q| Quantile {
                quantile: q.quantile,
                value: q.value,
            })
            .collect();

        MetricEvent::new(
            metric_name,
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles,
                count: self.point.count,
                sum: self.point.sum,
            },
        )
        .with_timestamp(timestamp)
        .with_tags(Some(attributes))
        .into()
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

// ============================================================================
// Native Vector Metric → OTLP Conversion
// ============================================================================

/// Convert a native Vector Metric to OTLP ExportMetricsServiceRequest.
///
/// This is the inverse of `ResourceMetrics::into_event_iter()` — it reconstructs
/// the OTLP protobuf structure from Vector's flat metric representation.
///
/// Tag decomposition (reverse of `build_metric_tags`):
/// - `resource.*` prefix → resource attributes (prefix stripped)
/// - `scope.name` → InstrumentationScope name
/// - `scope.version` → InstrumentationScope version
/// - `scope.*` prefix → scope attributes (prefix stripped)
/// - All other tags → data point attributes
///
/// MetricValue mapping:
/// - Counter → Sum (is_monotonic: true)
/// - Gauge → Gauge
/// - AggregatedHistogram → Histogram
/// - AggregatedSummary → Summary
/// - Distribution → Histogram (samples converted to buckets)
/// - Set → Gauge (count of unique values)
/// - Sketch → unsupported (warning emitted, metric dropped)
pub fn native_metric_to_otlp_request(metric: &MetricEvent) -> ExportMetricsServiceRequest {
    let (resource, scope, attributes) = decompose_metric_tags(metric.tags());
    let timestamp_nanos = extract_metric_timestamp_nanos(metric);
    let otlp_metric = build_otlp_metric(metric, &attributes, timestamp_nanos);

    let scope_metrics = ScopeMetrics {
        scope,
        metrics: vec![otlp_metric],
        schema_url: String::new(),
    };

    let resource_metrics = ResourceMetrics {
        resource,
        scope_metrics: vec![scope_metrics],
        schema_url: String::new(),
    };

    ExportMetricsServiceRequest {
        resource_metrics: vec![resource_metrics],
    }
}

/// Decompose Vector MetricTags back into OTLP resource, scope, and data point attributes.
///
/// This reverses the flattening done by `build_metric_tags` during decode:
/// - Tags with "resource." prefix → Resource attributes (prefix removed)
/// - "scope.name" → InstrumentationScope.name
/// - "scope.version" → InstrumentationScope.version
/// - Tags with "scope." prefix (other than name/version) → scope attributes (prefix removed)
/// - All other tags → data point KeyValue attributes
fn decompose_metric_tags(
    tags: Option<&MetricTags>,
) -> (
    Option<Resource>,
    Option<InstrumentationScope>,
    Vec<KeyValue>,
) {
    let tags = match tags {
        Some(t) => t,
        None => return (None, None, Vec::new()),
    };

    let mut resource_attrs = Vec::new();
    let mut scope_name: Option<String> = None;
    let mut scope_version: Option<String> = None;
    let mut scope_attrs = Vec::new();
    let mut data_point_attrs = Vec::new();

    for (key, value) in tags.iter_single() {
        let pb_value = PBValue::StringValue(value.to_string());
        let any_value = Some(AnyValue {
            value: Some(pb_value),
        });

        if let Some(resource_key) = key.strip_prefix("resource.") {
            resource_attrs.push(KeyValue {
                key: resource_key.to_string(),
                value: any_value,
            });
        } else if key == "scope.name" {
            scope_name = Some(value.to_string());
        } else if key == "scope.version" {
            scope_version = Some(value.to_string());
        } else if let Some(scope_key) = key.strip_prefix("scope.") {
            scope_attrs.push(KeyValue {
                key: scope_key.to_string(),
                value: any_value,
            });
        } else {
            data_point_attrs.push(KeyValue {
                key: key.to_string(),
                value: any_value,
            });
        }
    }

    let resource = if !resource_attrs.is_empty() {
        Some(Resource {
            attributes: resource_attrs,
            dropped_attributes_count: 0,
        })
    } else {
        None
    };

    let scope = if scope_name.is_some() || scope_version.is_some() || !scope_attrs.is_empty() {
        Some(InstrumentationScope {
            name: scope_name.unwrap_or_default(),
            version: scope_version.unwrap_or_default(),
            attributes: scope_attrs,
            dropped_attributes_count: 0,
        })
    } else {
        None
    };

    (resource, scope, data_point_attrs)
}

/// Extract timestamp as nanoseconds from a Metric event.
fn extract_metric_timestamp_nanos(metric: &MetricEvent) -> u64 {
    metric
        .timestamp()
        .and_then(|ts| ts.timestamp_nanos_opt())
        .and_then(|n| u64::try_from(n).ok())
        .unwrap_or(0)
}

/// Build the OTLP Metric protobuf message from a Vector MetricEvent.
fn build_otlp_metric(
    metric: &MetricEvent,
    attributes: &[KeyValue],
    timestamp_nanos: u64,
) -> super::proto::metrics::v1::Metric {
    let data = match metric.value() {
        MetricValue::Counter { value } => {
            let temporality = match metric.kind() {
                MetricKind::Incremental => AggregationTemporality::Delta as i32,
                MetricKind::Absolute => AggregationTemporality::Cumulative as i32,
            };
            Some(Data::Sum(Sum {
                data_points: vec![NumberDataPoint {
                    attributes: attributes.to_vec(),
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_nanos,
                    value: Some(NumberDataPointValue::AsDouble(*value)),
                    exemplars: Vec::new(),
                    flags: 0,
                }],
                aggregation_temporality: temporality,
                is_monotonic: true,
            }))
        }

        MetricValue::Gauge { value } => Some(Data::Gauge(Gauge {
            data_points: vec![NumberDataPoint {
                attributes: attributes.to_vec(),
                start_time_unix_nano: 0,
                time_unix_nano: timestamp_nanos,
                value: Some(NumberDataPointValue::AsDouble(*value)),
                exemplars: Vec::new(),
                flags: 0,
            }],
        })),

        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            let temporality = match metric.kind() {
                MetricKind::Incremental => AggregationTemporality::Delta as i32,
                MetricKind::Absolute => AggregationTemporality::Cumulative as i32,
            };

            // OTLP histogram: explicit_bounds has N-1 entries, bucket_counts has N entries.
            // The last bucket is implicitly [last_bound, +inf).
            // Vector stores each bucket with its own upper_limit, where the last
            // one typically has upper_limit = +inf.
            let mut explicit_bounds = Vec::with_capacity(buckets.len().saturating_sub(1));
            let mut bucket_counts = Vec::with_capacity(buckets.len());

            for bucket in buckets {
                bucket_counts.push(bucket.count);
                if bucket.upper_limit.is_finite() {
                    explicit_bounds.push(bucket.upper_limit);
                }
                // The +inf bucket is implicit in OTLP — its count is included
                // but its bound is not listed in explicit_bounds
            }

            Some(Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: attributes.to_vec(),
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_nanos,
                    count: *count,
                    sum: Some(*sum),
                    bucket_counts,
                    explicit_bounds,
                    exemplars: Vec::new(),
                    flags: 0,
                    min: None,
                    max: None,
                }],
                aggregation_temporality: temporality,
            }))
        }

        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            let quantile_values: Vec<ValueAtQuantile> = quantiles
                .iter()
                .map(|q| ValueAtQuantile {
                    quantile: q.quantile,
                    value: q.value,
                })
                .collect();

            Some(Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    attributes: attributes.to_vec(),
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_nanos,
                    count: *count,
                    sum: *sum,
                    quantile_values,
                    flags: 0,
                }],
            }))
        }

        MetricValue::Distribution { samples, statistic } => {
            // Convert distribution samples to an OTLP histogram.
            // Build histogram buckets from individual samples.
            let temporality = match metric.kind() {
                MetricKind::Incremental => AggregationTemporality::Delta as i32,
                MetricKind::Absolute => AggregationTemporality::Cumulative as i32,
            };

            // Collect unique sorted boundaries from sample values
            let mut boundaries: Vec<f64> = samples
                .iter()
                .map(|s| s.value)
                .filter(|v| v.is_finite())
                .collect();
            boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            boundaries.dedup();

            // Build bucket counts using the boundaries
            let mut bucket_counts = vec![0u64; boundaries.len() + 1];
            let mut total_count = 0u64;
            let mut total_sum = 0.0f64;

            for sample in samples {
                // Skip non-finite samples (NaN, +Inf, -Inf) — they are not in
                // boundaries and would corrupt total_sum / bucket assignment.
                if !sample.value.is_finite() {
                    continue;
                }

                let rate = u64::from(sample.rate);
                total_count += rate;
                total_sum += sample.value * f64::from(sample.rate);

                // Find which bucket this sample belongs to.
                // Because boundaries is built from finite sample values (deduped),
                // every finite sample has an exact match → Ok(idx).
                let bucket_idx = match boundaries.binary_search_by(|b| {
                    b.partial_cmp(&sample.value)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }) {
                    Ok(idx) => idx,
                    Err(idx) => idx,
                };
                // bucket_counts has len boundaries+1, so bucket_idx is always in range.
                // Guard defensively: if out of range, place in the overflow (+inf) bucket.
                let target_idx = bucket_idx.min(bucket_counts.len().saturating_sub(1));
                bucket_counts[target_idx] += rate;
            }

            let _ = statistic; // StatisticKind doesn't affect OTLP encoding

            Some(Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: attributes.to_vec(),
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_nanos,
                    count: total_count,
                    sum: Some(total_sum),
                    bucket_counts,
                    explicit_bounds: boundaries,
                    exemplars: Vec::new(),
                    flags: 0,
                    min: None,
                    max: None,
                }],
                aggregation_temporality: temporality,
            }))
        }

        MetricValue::Set { values } => {
            // Encode set cardinality as a gauge (count of unique values)
            Some(Data::Gauge(Gauge {
                data_points: vec![NumberDataPoint {
                    attributes: attributes.to_vec(),
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_nanos,
                    #[allow(clippy::cast_precision_loss)]
                    value: Some(NumberDataPointValue::AsDouble(values.len() as f64)),
                    exemplars: Vec::new(),
                    flags: 0,
                }],
            }))
        }

        MetricValue::Sketch { .. } => {
            warn!(
                message = "Sketch metrics cannot be directly converted to OTLP format. Metric will be dropped.",
                metric_name = metric.name(),
                internal_log_rate_limit = true,
            );
            None
        }
    };

    super::proto::metrics::v1::Metric {
        name: metric.name().to_string(),
        description: String::new(),
        unit: String::new(),
        data,
    }
}

#[cfg(test)]
mod native_metric_conversion_tests {
    use super::*;
    use chrono::Utc;
    use vector_core::event::metric::Sample;

    fn make_counter(name: &str, value: f64, kind: MetricKind) -> MetricEvent {
        MetricEvent::new(name.to_string(), kind, MetricValue::Counter { value })
    }

    fn make_gauge(name: &str, value: f64) -> MetricEvent {
        MetricEvent::new(
            name.to_string(),
            MetricKind::Absolute,
            MetricValue::Gauge { value },
        )
    }

    #[test]
    fn test_empty_counter_produces_valid_otlp() {
        let metric = make_counter("test.counter", 42.0, MetricKind::Incremental);
        let request = native_metric_to_otlp_request(&metric);

        assert_eq!(request.resource_metrics.len(), 1);
        assert_eq!(request.resource_metrics[0].scope_metrics.len(), 1);
        assert_eq!(
            request.resource_metrics[0].scope_metrics[0].metrics.len(),
            1
        );
    }

    #[test]
    fn test_counter_incremental_to_sum_delta() {
        let metric = make_counter("http.requests", 100.0, MetricKind::Incremental);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        assert_eq!(otlp_metric.name, "http.requests");

        match &otlp_metric.data {
            Some(Data::Sum(sum)) => {
                assert!(sum.is_monotonic);
                assert_eq!(
                    sum.aggregation_temporality,
                    AggregationTemporality::Delta as i32
                );
                assert_eq!(sum.data_points.len(), 1);
                assert_eq!(
                    sum.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(100.0))
                );
            }
            other => panic!("Expected Sum, got {other:?}"),
        }
    }

    #[test]
    fn test_counter_absolute_to_sum_cumulative() {
        let metric = make_counter("http.total", 500.0, MetricKind::Absolute);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Sum(sum)) => {
                assert!(sum.is_monotonic);
                assert_eq!(
                    sum.aggregation_temporality,
                    AggregationTemporality::Cumulative as i32
                );
            }
            other => panic!("Expected Sum, got {other:?}"),
        }
    }

    #[test]
    fn test_gauge_conversion() {
        let metric = make_gauge("cpu.usage", 75.5);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        assert_eq!(otlp_metric.name, "cpu.usage");

        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(gauge.data_points.len(), 1);
                assert_eq!(
                    gauge.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(75.5))
                );
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_histogram_conversion() {
        let metric = MetricEvent::new(
            "request.duration".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 10.0,
                        count: 5,
                    },
                    Bucket {
                        upper_limit: 50.0,
                        count: 15,
                    },
                    Bucket {
                        upper_limit: 100.0,
                        count: 8,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 2,
                    },
                ],
                count: 30,
                sum: 1500.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                assert_eq!(
                    hist.aggregation_temporality,
                    AggregationTemporality::Cumulative as i32
                );
                let dp = &hist.data_points[0];
                assert_eq!(dp.count, 30);
                assert_eq!(dp.sum, Some(1500.0));
                // explicit_bounds should not include +inf
                assert_eq!(dp.explicit_bounds, vec![10.0, 50.0, 100.0]);
                // bucket_counts includes the +inf bucket
                assert_eq!(dp.bucket_counts, vec![5, 15, 8, 2]);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_summary_conversion() {
        let metric = MetricEvent::new(
            "request.latency".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 100.0,
                    },
                    Quantile {
                        quantile: 0.9,
                        value: 250.0,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 500.0,
                    },
                ],
                count: 1000,
                sum: 150000.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Summary(summary)) => {
                let dp = &summary.data_points[0];
                assert_eq!(dp.count, 1000);
                assert_eq!(dp.sum, 150000.0);
                assert_eq!(dp.quantile_values.len(), 3);
                assert_eq!(dp.quantile_values[0].quantile, 0.5);
                assert_eq!(dp.quantile_values[0].value, 100.0);
                assert_eq!(dp.quantile_values[2].quantile, 0.99);
                assert_eq!(dp.quantile_values[2].value, 500.0);
            }
            other => panic!("Expected Summary, got {other:?}"),
        }
    }

    #[test]
    fn test_tag_decomposition_resource() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace(
            "resource.service.name".to_string(),
            "my-service".to_string(),
        );
        tags.replace("resource.host.name".to_string(), "host1".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let resource = request.resource_metrics[0].resource.as_ref().unwrap();

        assert_eq!(resource.attributes.len(), 2);
        let keys: Vec<&str> = resource
            .attributes
            .iter()
            .map(|kv| kv.key.as_str())
            .collect();
        assert!(keys.contains(&"service.name"));
        assert!(keys.contains(&"host.name"));
    }

    #[test]
    fn test_tag_decomposition_scope() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("scope.name".to_string(), "my-meter".to_string());
        tags.replace("scope.version".to_string(), "2.0.0".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let scope = request.resource_metrics[0].scope_metrics[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "my-meter");
        assert_eq!(scope.version, "2.0.0");
    }

    #[test]
    fn test_tag_decomposition_data_point_attributes() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("http.method".to_string(), "GET".to_string());
        tags.replace("http.status_code".to_string(), "200".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                let attrs = &gauge.data_points[0].attributes;
                assert_eq!(attrs.len(), 2);
                let keys: Vec<&str> = attrs.iter().map(|kv| kv.key.as_str()).collect();
                assert!(keys.contains(&"http.method"));
                assert!(keys.contains(&"http.status_code"));
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_tag_decomposition_mixed() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("resource.service.name".to_string(), "svc".to_string());
        tags.replace("scope.name".to_string(), "meter".to_string());
        tags.replace("http.method".to_string(), "POST".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);

        // Resource should have service.name
        let resource = request.resource_metrics[0].resource.as_ref().unwrap();
        assert_eq!(resource.attributes.len(), 1);
        assert_eq!(resource.attributes[0].key, "service.name");

        // Scope should have name
        let scope = request.resource_metrics[0].scope_metrics[0]
            .scope
            .as_ref()
            .unwrap();
        assert_eq!(scope.name, "meter");

        // Data point should have http.method
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(gauge.data_points[0].attributes.len(), 1);
                assert_eq!(gauge.data_points[0].attributes[0].key, "http.method");
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_no_tags_produces_valid_otlp() {
        let metric = make_gauge("test", 1.0);
        let request = native_metric_to_otlp_request(&metric);

        assert!(request.resource_metrics[0].resource.is_none());
        assert!(request.resource_metrics[0].scope_metrics[0].scope.is_none());
    }

    #[test]
    fn test_timestamp_preserved() {
        let ts = Utc::now();
        let metric = make_gauge("test", 1.0).with_timestamp(Some(ts));
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                let expected_nanos = u64::try_from(ts.timestamp_nanos_opt().unwrap()).unwrap();
                assert_eq!(gauge.data_points[0].time_unix_nano, expected_nanos);
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_no_timestamp_produces_zero() {
        let metric = make_gauge("test", 1.0);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(gauge.data_points[0].time_unix_nano, 0);
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_set_converted_to_gauge_cardinality() {
        let metric = MetricEvent::new(
            "unique.users".to_string(),
            MetricKind::Absolute,
            MetricValue::Set {
                values: ["alice", "bob", "charlie"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(
                    gauge.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(3.0))
                );
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_converted_to_histogram() {
        let metric = MetricEvent::new(
            "request.size".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: 10.0,
                        rate: 3,
                    },
                    Sample {
                        value: 50.0,
                        rate: 2,
                    },
                    Sample {
                        value: 100.0,
                        rate: 1,
                    },
                ],
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                assert_eq!(
                    hist.aggregation_temporality,
                    AggregationTemporality::Delta as i32
                );
                let dp = &hist.data_points[0];
                // Total: 3*10 + 2*50 + 1*100 = 230
                assert_eq!(dp.sum, Some(230.0));
                // Total count: 3 + 2 + 1 = 6
                assert_eq!(dp.count, 6);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_histogram_incremental_to_delta() {
        let metric = MetricEvent::new(
            "hist.delta".to_string(),
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 10.0,
                        count: 5,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 2,
                    },
                ],
                count: 7,
                sum: 50.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                assert_eq!(
                    hist.aggregation_temporality,
                    AggregationTemporality::Delta as i32
                );
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_metric_name_preserved() {
        let metric = make_counter("my.service.requests.total", 42.0, MetricKind::Incremental);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        assert_eq!(otlp_metric.name, "my.service.requests.total");
    }

    #[test]
    fn test_scope_attributes() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("scope.name".to_string(), "meter".to_string());
        tags.replace("scope.custom_attr".to_string(), "custom_value".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let scope = request.resource_metrics[0].scope_metrics[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "meter");
        assert_eq!(scope.attributes.len(), 1);
        assert_eq!(scope.attributes[0].key, "custom_attr");
    }

    // ====================================================================
    // Roundtrip tests: encode → decode → verify fidelity
    // ====================================================================

    #[test]
    fn test_roundtrip_counter() {
        let mut original = make_counter("http.requests", 42.0, MetricKind::Incremental);
        let ts = Utc::now();
        original = original.with_timestamp(Some(ts));
        let mut tags = MetricTags::default();
        tags.replace("resource.service.name".to_string(), "web".to_string());
        tags.replace("http.method".to_string(), "GET".to_string());
        original = original.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&original);

        // Decode back
        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        assert_eq!(events.len(), 1);
        let decoded = events[0].as_metric();
        assert_eq!(decoded.name(), "http.requests");
        assert_eq!(decoded.kind(), MetricKind::Incremental);
        match decoded.value() {
            MetricValue::Counter { value } => assert_eq!(value, &42.0),
            other => panic!("Expected Counter, got {other:?}"),
        }

        // Verify tags roundtrip
        let decoded_tags = decoded.tags().unwrap();
        assert_eq!(decoded_tags.get("resource.service.name").unwrap(), "web");
        assert_eq!(decoded_tags.get("http.method").unwrap(), "GET");
    }

    #[test]
    fn test_roundtrip_gauge() {
        let metric = make_gauge("cpu.usage", 75.5).with_timestamp(Some(Utc::now()));
        let request = native_metric_to_otlp_request(&metric);

        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        assert_eq!(events.len(), 1);
        let decoded = events[0].as_metric();
        assert_eq!(decoded.name(), "cpu.usage");
        assert_eq!(decoded.kind(), MetricKind::Absolute);
        match decoded.value() {
            MetricValue::Gauge { value } => assert_eq!(value, &75.5),
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_roundtrip_histogram() {
        let original = MetricEvent::new(
            "request.duration".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 10.0,
                        count: 5,
                    },
                    Bucket {
                        upper_limit: 50.0,
                        count: 15,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 2,
                    },
                ],
                count: 22,
                sum: 800.0,
            },
        )
        .with_timestamp(Some(Utc::now()));

        let request = native_metric_to_otlp_request(&original);
        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        assert_eq!(events.len(), 1);
        let decoded = events[0].as_metric();
        assert_eq!(decoded.name(), "request.duration");
        match decoded.value() {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                assert_eq!(count, &22);
                assert_eq!(sum, &800.0);
                assert_eq!(buckets.len(), 3);
                assert_eq!(buckets[0].upper_limit, 10.0);
                assert_eq!(buckets[0].count, 5);
                assert_eq!(buckets[1].upper_limit, 50.0);
                assert_eq!(buckets[1].count, 15);
                assert!(buckets[2].upper_limit.is_infinite());
                assert_eq!(buckets[2].count, 2);
            }
            other => panic!("Expected AggregatedHistogram, got {other:?}"),
        }
    }

    #[test]
    fn test_roundtrip_summary() {
        let original = MetricEvent::new(
            "latency".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 100.0,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 500.0,
                    },
                ],
                count: 1000,
                sum: 100000.0,
            },
        )
        .with_timestamp(Some(Utc::now()));

        let request = native_metric_to_otlp_request(&original);
        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        assert_eq!(events.len(), 1);
        let decoded = events[0].as_metric();
        match decoded.value() {
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                assert_eq!(count, &1000);
                assert_eq!(sum, &100000.0);
                assert_eq!(quantiles.len(), 2);
                assert_eq!(quantiles[0].quantile, 0.5);
                assert_eq!(quantiles[0].value, 100.0);
            }
            other => panic!("Expected AggregatedSummary, got {other:?}"),
        }
    }

    #[test]
    fn test_roundtrip_counter_absolute_cumulative() {
        let metric = make_counter("total.bytes", 999.0, MetricKind::Absolute);
        let request = native_metric_to_otlp_request(&metric);

        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        let decoded = events[0].as_metric();
        assert_eq!(decoded.kind(), MetricKind::Absolute);
        match decoded.value() {
            MetricValue::Counter { value } => assert_eq!(value, &999.0),
            other => panic!("Expected Counter, got {other:?}"),
        }
    }

    // ====================================================================
    // Edge case tests
    // ====================================================================

    #[test]
    fn test_counter_zero_value() {
        let metric = make_counter("zero.counter", 0.0, MetricKind::Incremental);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Sum(sum)) => {
                assert_eq!(
                    sum.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(0.0))
                );
            }
            other => panic!("Expected Sum, got {other:?}"),
        }
    }

    #[test]
    fn test_gauge_negative_value() {
        let metric = make_gauge("temperature", -15.5);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(
                    gauge.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(-15.5))
                );
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_gauge_very_large_value() {
        let metric = make_gauge("big.value", f64::MAX);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(
                    gauge.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(f64::MAX))
                );
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_histogram_empty_buckets() {
        let metric = MetricEvent::new(
            "empty.hist".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: Vec::new(),
                count: 0,
                sum: 0.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                assert!(dp.bucket_counts.is_empty());
                assert!(dp.explicit_bounds.is_empty());
                assert_eq!(dp.count, 0);
                assert_eq!(dp.sum, Some(0.0));
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_summary_empty_quantiles() {
        let metric = MetricEvent::new(
            "empty.summary".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: Vec::new(),
                count: 0,
                sum: 0.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Summary(summary)) => {
                let dp = &summary.data_points[0];
                assert!(dp.quantile_values.is_empty());
                assert_eq!(dp.count, 0);
                assert_eq!(dp.sum, 0.0);
            }
            other => panic!("Expected Summary, got {other:?}"),
        }
    }

    #[test]
    fn test_set_empty_produces_gauge_zero() {
        let metric = MetricEvent::new(
            "empty.set".to_string(),
            MetricKind::Absolute,
            MetricValue::Set {
                values: std::collections::BTreeSet::new(),
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => {
                assert_eq!(
                    gauge.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(0.0))
                );
            }
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_empty_samples() {
        let metric = MetricEvent::new(
            "empty.dist".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: Vec::new(),
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                assert_eq!(dp.count, 0);
                assert_eq!(dp.sum, Some(0.0));
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_single_sample() {
        let metric = MetricEvent::new(
            "single.dist".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![Sample {
                    value: 42.0,
                    rate: 1,
                }],
                statistic: vector_core::event::metric::StatisticKind::Summary,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                assert_eq!(dp.count, 1);
                assert_eq!(dp.sum, Some(42.0));
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_multiple_resource_attributes() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("resource.service.name".to_string(), "api".to_string());
        tags.replace("resource.service.version".to_string(), "1.0".to_string());
        tags.replace(
            "resource.deployment.environment".to_string(),
            "prod".to_string(),
        );
        tags.replace("resource.host.name".to_string(), "host-1".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let resource = request.resource_metrics[0].resource.as_ref().unwrap();

        assert_eq!(resource.attributes.len(), 4);
        let keys: Vec<&str> = resource
            .attributes
            .iter()
            .map(|kv| kv.key.as_str())
            .collect();
        assert!(keys.contains(&"service.name"));
        assert!(keys.contains(&"service.version"));
        assert!(keys.contains(&"deployment.environment"));
        assert!(keys.contains(&"host.name"));
    }

    #[test]
    fn test_scope_version_only() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("scope.version".to_string(), "3.0.0".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let scope = request.resource_metrics[0].scope_metrics[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "");
        assert_eq!(scope.version, "3.0.0");
    }

    #[test]
    fn test_histogram_single_bucket_plus_infinity() {
        let metric = MetricEvent::new(
            "simple.hist".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: f64::INFINITY,
                    count: 10,
                }],
                count: 10,
                sum: 100.0,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                // Only +inf bucket → explicit_bounds should be empty
                assert!(dp.explicit_bounds.is_empty());
                assert_eq!(dp.bucket_counts, vec![10]);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_counter_large_value_roundtrip() {
        let metric = make_counter("big.counter", 1e18, MetricKind::Incremental)
            .with_timestamp(Some(Utc::now()));
        let request = native_metric_to_otlp_request(&metric);

        let events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        let decoded = events[0].as_metric();
        match decoded.value() {
            MetricValue::Counter { value } => assert_eq!(value, &1e18),
            other => panic!("Expected Counter, got {other:?}"),
        }
    }

    #[test]
    fn test_metric_name_with_special_characters() {
        let metric = make_gauge("my-service.cpu_usage.percent", 50.0);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        assert_eq!(otlp_metric.name, "my-service.cpu_usage.percent");
    }

    // ====================================================================
    // NaN / Infinity edge case tests
    // ====================================================================

    #[test]
    fn test_distribution_nan_samples_are_skipped() {
        let metric = MetricEvent::new(
            "dist.nan".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: 10.0,
                        rate: 2,
                    },
                    Sample {
                        value: f64::NAN,
                        rate: 1,
                    },
                    Sample {
                        value: 20.0,
                        rate: 3,
                    },
                ],
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                // NaN sample skipped: count = 2 + 3 = 5, sum = 10*2 + 20*3 = 80
                assert_eq!(dp.count, 5);
                assert_eq!(dp.sum, Some(80.0));
                // Boundaries from finite samples only: [10.0, 20.0]
                assert_eq!(dp.explicit_bounds, vec![10.0, 20.0]);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_infinity_samples_are_skipped() {
        let metric = MetricEvent::new(
            "dist.inf".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: 5.0,
                        rate: 1,
                    },
                    Sample {
                        value: f64::INFINITY,
                        rate: 1,
                    },
                    Sample {
                        value: f64::NEG_INFINITY,
                        rate: 1,
                    },
                ],
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                // Only the finite sample counted
                assert_eq!(dp.count, 1);
                assert_eq!(dp.sum, Some(5.0));
                assert_eq!(dp.explicit_bounds, vec![5.0]);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_all_nan_produces_empty_histogram() {
        let metric = MetricEvent::new(
            "dist.all_nan".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: f64::NAN,
                        rate: 1,
                    },
                    Sample {
                        value: f64::NAN,
                        rate: 2,
                    },
                ],
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                assert_eq!(dp.count, 0);
                assert_eq!(dp.sum, Some(0.0));
                assert!(dp.explicit_bounds.is_empty());
                // Only the overflow bucket exists
                assert_eq!(dp.bucket_counts, vec![0]);
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_gauge_nan_value_passes_through() {
        let metric = make_gauge("nan.gauge", f64::NAN);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Gauge(gauge)) => match &gauge.data_points[0].value {
                Some(NumberDataPointValue::AsDouble(v)) => assert!(v.is_nan()),
                other => panic!("Expected AsDouble(NaN), got {other:?}"),
            },
            other => panic!("Expected Gauge, got {other:?}"),
        }
    }

    #[test]
    fn test_counter_infinity_value_passes_through() {
        let metric = make_counter("inf.counter", f64::INFINITY, MetricKind::Incremental);
        let request = native_metric_to_otlp_request(&metric);

        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];
        match &otlp_metric.data {
            Some(Data::Sum(sum)) => {
                assert_eq!(
                    sum.data_points[0].value,
                    Some(NumberDataPointValue::AsDouble(f64::INFINITY))
                );
            }
            other => panic!("Expected Sum, got {other:?}"),
        }
    }

    #[test]
    fn test_distribution_with_zero_rate_samples() {
        let metric = MetricEvent::new(
            "dist.zero_rate".to_string(),
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: 10.0,
                        rate: 0,
                    },
                    Sample {
                        value: 20.0,
                        rate: 2,
                    },
                ],
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                // rate=0 contributes 0 to count and sum
                assert_eq!(dp.count, 2);
                assert_eq!(dp.sum, Some(40.0));
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    // ====================================================================
    // Sketch handling tests
    // ====================================================================

    #[test]
    fn test_sketch_produces_no_data() {
        use vector_core::event::metric::MetricSketch;
        use vector_core::metrics::AgentDDSketch;

        let sketch = AgentDDSketch::with_agent_defaults();
        let metric = MetricEvent::new(
            "sketch.metric".to_string(),
            MetricKind::Absolute,
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(sketch),
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        // Sketch metrics should produce a metric with no data (dropped)
        assert_eq!(otlp_metric.name, "sketch.metric");
        assert!(otlp_metric.data.is_none());
    }

    // ====================================================================
    // Tag prefix collision tests
    // ====================================================================

    #[test]
    fn test_tag_with_resource_prefix_routed_to_resource() {
        // A data-point attribute originally named "resource.host" gets stored
        // as tag "resource.host" during decode. On re-encode, it becomes a
        // resource attribute with key "host". This is a known limitation of
        // the prefix-based decomposition approach.
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("resource.host".to_string(), "localhost".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let resource = request.resource_metrics[0].resource.as_ref().unwrap();

        assert_eq!(resource.attributes.len(), 1);
        assert_eq!(resource.attributes[0].key, "host");
    }

    #[test]
    fn test_scope_custom_attributes_preserved() {
        let metric = make_gauge("test", 1.0);
        let mut tags = MetricTags::default();
        tags.replace("scope.name".to_string(), "meter".to_string());
        tags.replace("scope.custom.key".to_string(), "custom_val".to_string());
        tags.replace("scope.another.attr".to_string(), "another_val".to_string());
        let metric = metric.with_tags(Some(tags));

        let request = native_metric_to_otlp_request(&metric);
        let scope = request.resource_metrics[0].scope_metrics[0]
            .scope
            .as_ref()
            .unwrap();

        assert_eq!(scope.name, "meter");
        assert_eq!(scope.attributes.len(), 2);
        let keys: Vec<&str> = scope.attributes.iter().map(|kv| kv.key.as_str()).collect();
        assert!(keys.contains(&"custom.key"));
        assert!(keys.contains(&"another.attr"));
    }

    // ====================================================================
    // Histogram non-finite sum passthrough tests
    // ====================================================================

    #[test]
    fn test_aggregated_histogram_nan_sum_passes_through() {
        let metric = MetricEvent::new(
            "hist.nan_sum".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: 10.0,
                    count: 1,
                }],
                count: 1,
                sum: f64::NAN,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Histogram(hist)) => {
                let dp = &hist.data_points[0];
                assert!(dp.sum.unwrap().is_nan());
            }
            other => panic!("Expected Histogram, got {other:?}"),
        }
    }

    #[test]
    fn test_aggregated_summary_nan_sum_passes_through() {
        let metric = MetricEvent::new(
            "summary.nan_sum".to_string(),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![Quantile {
                    quantile: 0.5,
                    value: 100.0,
                }],
                count: 10,
                sum: f64::NAN,
            },
        );

        let request = native_metric_to_otlp_request(&metric);
        let otlp_metric = &request.resource_metrics[0].scope_metrics[0].metrics[0];

        match &otlp_metric.data {
            Some(Data::Summary(summary)) => {
                let dp = &summary.data_points[0];
                assert!(dp.sum.is_nan());
            }
            other => panic!("Expected Summary, got {other:?}"),
        }
    }
}

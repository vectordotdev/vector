use chrono::{TimeZone, Utc};
use vector_core::event::{
    Event, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
    metric::{Bucket, Quantile, TagValue, TagValueSet},
};

use super::common::str_to_key_value;
use super::proto::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::{InstrumentationScope, KeyValue},
    metrics::v1::{
        AggregationTemporality, ExponentialHistogram, ExponentialHistogramDataPoint, Gauge,
        Histogram, HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
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

/// The single/last value of a tag, preserving `TagValue::Bare` (unlike `MetricTags::iter_single`,
/// which discards `Bare` tags because it goes through `TagValue::as_option`).
fn representative_tag_value(tag_set: &TagValueSet) -> Option<TagValue> {
    match tag_set {
        TagValueSet::Empty => None,
        TagValueSet::Single(tag) => Some(tag.clone()),
        TagValueSet::Set(set) => set.iter().last().cloned(),
    }
}

/// Splits a metric's tags back into the `Resource`, `InstrumentationScope`, and data point
/// `attributes` they were flattened from by [`build_metric_tags`].
pub fn split_metric_tags(tags: &MetricTags) -> (Resource, InstrumentationScope, Vec<KeyValue>) {
    let mut resource_attributes = Vec::new();
    let mut scope_name = String::new();
    let mut scope_version = String::new();
    let mut scope_attributes = Vec::new();
    let mut attributes = Vec::new();

    for (key, tag_set) in tags.iter_sets() {
        let Some(value) = representative_tag_value(tag_set) else {
            continue;
        };

        if let Some(rest) = key.strip_prefix("resource.") {
            resource_attributes.push(str_to_key_value(rest, &value));
        } else if key == "scope.name" {
            scope_name = value.as_option().unwrap_or_default().to_string();
        } else if key == "scope.version" {
            scope_version = value.as_option().unwrap_or_default().to_string();
        } else if let Some(rest) = key.strip_prefix("scope.") {
            scope_attributes.push(str_to_key_value(rest, &value));
        } else {
            attributes.push(str_to_key_value(key, &value));
        }
    }

    let resource = Resource {
        attributes: resource_attributes,
        dropped_attributes_count: 0,
    };
    let scope = InstrumentationScope {
        name: scope_name,
        version: scope_version,
        attributes: scope_attributes,
        dropped_attributes_count: 0,
    };

    (resource, scope, attributes)
}

fn metric_value_to_data(
    value: &MetricValue,
    kind: MetricKind,
    timestamp_ns: u64,
    attrs: Vec<KeyValue>,
) -> Result<Data, vector_common::Error> {
    let temporality = match kind {
        MetricKind::Incremental => AggregationTemporality::Delta,
        MetricKind::Absolute => AggregationTemporality::Cumulative,
    } as i32;

    match value {
        MetricValue::Counter { value } => Ok(Data::Sum(Sum {
            data_points: vec![NumberDataPoint {
                attributes: attrs,
                start_time_unix_nano: 0,
                time_unix_nano: timestamp_ns,
                value: Some(NumberDataPointValue::AsDouble(*value)),
                exemplars: Vec::new(),
                flags: 0,
            }],
            aggregation_temporality: temporality,
            is_monotonic: true,
        })),
        MetricValue::Gauge { value } => Ok(Data::Gauge(Gauge {
            data_points: vec![NumberDataPoint {
                attributes: attrs,
                start_time_unix_nano: 0,
                time_unix_nano: timestamp_ns,
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
            // The decode path treats the last bucket's `upper_limit` as `+Inf`, folding it into
            // `bucket_counts` without a matching `explicit_bounds` entry. Mirror that here: emit
            // all `N` counts but only the first `N-1` bounds, or OTLP receivers reject the point.
            let bucket_counts: Vec<u64> = buckets.iter().map(|bucket| bucket.count).collect();
            let explicit_bounds: Vec<f64> = buckets
                .iter()
                .take(buckets.len().saturating_sub(1))
                .map(|bucket| bucket.upper_limit)
                .collect();

            Ok(Data::Histogram(Histogram {
                data_points: vec![HistogramDataPoint {
                    attributes: attrs,
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_ns,
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
            let quantile_values = quantiles
                .iter()
                .map(|quantile| ValueAtQuantile {
                    quantile: quantile.quantile,
                    value: quantile.value,
                })
                .collect();

            Ok(Data::Summary(Summary {
                data_points: vec![SummaryDataPoint {
                    attributes: attrs,
                    start_time_unix_nano: 0,
                    time_unix_nano: timestamp_ns,
                    count: *count,
                    sum: *sum,
                    quantile_values,
                    flags: 0,
                }],
            }))
        }
        MetricValue::Set { .. } => {
            Err("OTLP serializer does not support Set (statsd-style) metric values".into())
        }
        MetricValue::Distribution { .. } => Err(
            "OTLP serializer does not support Distribution (un-aggregated) metric values".into(),
        ),
        MetricValue::Sketch { .. } => {
            Err("OTLP serializer does not support Sketch (DDSketch) metric values".into())
        }
    }
}

/// Converts a native Vector `Metric` event into an OTLP `ExportMetricsServiceRequest`.
///
/// This is the inverse of `ResourceMetrics::into_event_iter`. See the module-level docs for
/// which `MetricValue` variants are supported and why the reverse direction is not lossless.
pub fn metric_event_to_export_request(
    metric: MetricEvent,
) -> Result<ExportMetricsServiceRequest, vector_common::Error> {
    let timestamp_ns = metric
        .timestamp()
        .and_then(|ts| ts.timestamp_nanos_opt())
        .unwrap_or_else(|| Utc::now().timestamp_nanos_opt().unwrap_or(0))
        as u64;

    let empty_tags = MetricTags::default();
    let tags = metric.tags().unwrap_or(&empty_tags);
    let (resource, scope, attributes) = split_metric_tags(tags);

    let kind = metric.kind();
    let name = metric.name().to_string();
    let data = metric_value_to_data(metric.value(), kind, timestamp_ns, attributes)?;

    Ok(ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(resource),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(scope),
                metrics: vec![Metric {
                    name,
                    description: String::new(),
                    unit: String::new(),
                    data: Some(data),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::super::proto::common::v1::any_value::Value as PBValue;
    use super::*;
    use vector_core::event::MetricValue;
    use vector_core::event::metric::StatisticKind;

    fn number_data_point(data: Data) -> NumberDataPoint {
        match data {
            Data::Sum(sum) => sum.data_points.into_iter().next().unwrap(),
            Data::Gauge(gauge) => gauge.data_points.into_iter().next().unwrap(),
            other => panic!("expected a number data point, got {other:?}"),
        }
    }

    #[test]
    fn counter_to_otlp_sum() {
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        let data = metric_value_to_data(metric.value(), metric.kind(), 1_000, Vec::new())
            .expect("counter should encode");

        match data {
            Data::Sum(sum) => {
                assert!(sum.is_monotonic);
                assert_eq!(
                    sum.aggregation_temporality,
                    AggregationTemporality::Delta as i32
                );
                let point = sum.data_points.into_iter().next().unwrap();
                assert_eq!(point.value, Some(NumberDataPointValue::AsDouble(42.0)));
                assert_eq!(point.time_unix_nano, 1_000);
            }
            other => panic!("expected Data::Sum, got {other:?}"),
        }

        // Absolute kind maps to Cumulative temporality.
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );
        let data = metric_value_to_data(metric.value(), metric.kind(), 1, Vec::new()).unwrap();
        match data {
            Data::Sum(sum) => assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            ),
            other => panic!("expected Data::Sum, got {other:?}"),
        }
    }

    #[test]
    fn gauge_to_otlp_gauge() {
        let attrs = vec![str_to_key_value("host", &TagValue::from("localhost"))];
        let metric = MetricEvent::new(
            "cpu",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 12.5 },
        );

        let data = metric_value_to_data(metric.value(), metric.kind(), 5, attrs.clone()).unwrap();
        let point = number_data_point(data);
        assert_eq!(point.value, Some(NumberDataPointValue::AsDouble(12.5)));
        assert_eq!(point.attributes, attrs);
    }

    #[test]
    fn aggregated_histogram_to_otlp_histogram() {
        let buckets = vec![
            Bucket {
                upper_limit: 1.0,
                count: 1,
            },
            Bucket {
                upper_limit: 2.0,
                count: 2,
            },
            Bucket {
                upper_limit: f64::INFINITY,
                count: 3,
            },
        ];
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets,
                count: 6,
                sum: 10.0,
            },
        );

        let data = metric_value_to_data(metric.value(), metric.kind(), 42, Vec::new()).unwrap();
        match data {
            Data::Histogram(histogram) => {
                let point = histogram.data_points.into_iter().next().unwrap();
                assert_eq!(point.bucket_counts, vec![1, 2, 3]);
                // Trailing +Inf bound must be dropped: N counts, N-1 bounds.
                assert_eq!(point.explicit_bounds, vec![1.0, 2.0]);
                assert_eq!(point.count, 6);
                assert_eq!(point.sum, Some(10.0));
            }
            other => panic!("expected Data::Histogram, got {other:?}"),
        }
    }

    #[test]
    fn aggregated_summary_to_otlp_summary() {
        let quantiles = vec![
            Quantile {
                quantile: 0.5,
                value: 10.0,
            },
            Quantile {
                quantile: 0.99,
                value: 20.0,
            },
        ];
        let metric = MetricEvent::new(
            "response_time",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles,
                count: 100,
                sum: 1000.0,
            },
        );

        let data = metric_value_to_data(metric.value(), metric.kind(), 1, Vec::new()).unwrap();
        match data {
            Data::Summary(summary) => {
                let point = summary.data_points.into_iter().next().unwrap();
                assert_eq!(point.count, 100);
                assert_eq!(point.sum, 1000.0);
                assert_eq!(
                    point.quantile_values,
                    vec![
                        ValueAtQuantile {
                            quantile: 0.5,
                            value: 10.0
                        },
                        ValueAtQuantile {
                            quantile: 0.99,
                            value: 20.0
                        },
                    ]
                );
            }
            other => panic!("expected Data::Summary, got {other:?}"),
        }
    }

    #[test]
    fn tag_splitting() {
        let mut tags = MetricTags::default();
        tags.insert("resource.service.name".to_string(), "my-service");
        tags.insert("scope.name".to_string(), "my-scope");
        tags.insert("scope.version".to_string(), "1.0.0");
        tags.insert("scope.custom".to_string(), "scope-value");
        tags.insert("env".to_string(), "prod");
        tags.insert("bare_tag".to_string(), TagValue::Bare);

        let (resource, scope, attributes) = split_metric_tags(&tags);

        assert_eq!(resource.attributes.len(), 1);
        assert_eq!(resource.attributes[0].key, "service.name");
        assert_eq!(
            resource.attributes[0].value.as_ref().unwrap().value,
            Some(PBValue::StringValue("my-service".to_string()))
        );

        assert_eq!(scope.name, "my-scope");
        assert_eq!(scope.version, "1.0.0");
        assert_eq!(scope.attributes.len(), 1);
        assert_eq!(scope.attributes[0].key, "custom");

        assert_eq!(attributes.len(), 2);
        let env = attributes.iter().find(|kv| kv.key == "env").unwrap();
        assert_eq!(
            env.value.as_ref().unwrap().value,
            Some(PBValue::StringValue("prod".to_string()))
        );
        let bare = attributes.iter().find(|kv| kv.key == "bare_tag").unwrap();
        assert_eq!(
            bare.value.as_ref().unwrap().value,
            Some(PBValue::StringValue(String::new()))
        );
    }

    #[test]
    fn unsupported_metric_returns_err() {
        let set_metric = MetricEvent::new(
            "unique_users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: std::iter::once("a".to_string()).collect(),
            },
        );
        let err = metric_event_to_export_request(set_metric).unwrap_err();
        assert!(err.to_string().contains("Set"));

        let distribution_metric = MetricEvent::new(
            "latencies",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: Vec::new(),
                statistic: StatisticKind::Histogram,
            },
        );
        let err = metric_event_to_export_request(distribution_metric).unwrap_err();
        assert!(err.to_string().contains("Distribution"));
    }

    #[test]
    fn timestamp_fallback() {
        let metric = MetricEvent::new(
            "cpu",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        );
        assert!(metric.timestamp().is_none());

        let request = metric_event_to_export_request(metric).unwrap();
        let data = request.resource_metrics[0].scope_metrics[0].metrics[0]
            .data
            .clone()
            .unwrap();
        let point = number_data_point(data);
        assert_ne!(point.time_unix_nano, 0);
    }
}

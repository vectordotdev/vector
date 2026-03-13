use chrono::{TimeZone, Utc};
use vector_core::event::{
    Event, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
    metric::{Bucket, Quantile, TagValue},
};

use super::proto::{
    common::v1::{InstrumentationScope, KeyValue},
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
                "resource.dropped_attributes_count".to_string(),
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
                "scope.dropped_attributes_count".to_string(),
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

    if !scope_schema_url.is_empty() {
        tags.insert("scope.schema_url".to_string(), scope_schema_url.to_string());
    }
    if !resource_schema_url.is_empty() {
        tags.insert(
            "resource.schema_url".to_string(),
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

        MetricEvent::new(metric_name, kind, metric_value)
            .with_tags(Some(attributes))
            .with_timestamp(timestamp)
            .into()
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
        let attributes = build_metric_tags(
            self.resource,
            self.scope,
            &self.point.attributes,
            scope_schema_url,
            resource_schema_url,
        );

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

    // ========================================================================
    // Tests for schema_url tags
    // ========================================================================

    #[test]
    fn test_scope_schema_url_as_tag() {
        let rm = make_resource_metrics(vec![], 0, None, "https://scope.schema", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        assert_eq!(events.len(), 1);
        let tags = get_tags(&events[0]);
        assert_eq!(
            tags.get("scope.schema_url").unwrap(),
            "https://scope.schema"
        );
    }

    #[test]
    fn test_resource_schema_url_as_tag() {
        let rm = make_resource_metrics(vec![], 0, None, "", "https://resource.schema");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert_eq!(
            tags.get("resource.schema_url").unwrap(),
            "https://resource.schema"
        );
    }

    #[test]
    fn test_empty_schema_urls_not_tagged() {
        let rm = make_resource_metrics(vec![], 0, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert!(tags.get("scope.schema_url").is_none());
        assert!(tags.get("resource.schema_url").is_none());
    }

    // ========================================================================
    // Tests for dropped_attributes_count tags
    // ========================================================================

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
        assert_eq!(tags.get("scope.dropped_attributes_count").unwrap(), "5");
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
        assert!(tags.get("scope.dropped_attributes_count").is_none());
    }

    #[test]
    fn test_resource_dropped_attributes_count_tag() {
        let rm = make_resource_metrics(vec![make_kv("host", "server")], 3, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert_eq!(tags.get("resource.dropped_attributes_count").unwrap(), "3");
    }

    #[test]
    fn test_resource_dropped_zero_not_tagged() {
        let rm = make_resource_metrics(vec![make_kv("host", "server")], 0, None, "", "");
        let events: Vec<Event> = rm.into_event_iter().collect();
        let tags = get_tags(&events[0]);
        assert!(tags.get("resource.dropped_attributes_count").is_none());
    }

    // ========================================================================
    // Combined: all new tags
    // ========================================================================

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
            tags.get("scope.schema_url").unwrap(),
            "https://scope.schema"
        );
        assert_eq!(
            tags.get("resource.schema_url").unwrap(),
            "https://resource.schema"
        );
        assert_eq!(tags.get("scope.dropped_attributes_count").unwrap(), "1");
        assert_eq!(tags.get("resource.dropped_attributes_count").unwrap(), "2");
    }
}

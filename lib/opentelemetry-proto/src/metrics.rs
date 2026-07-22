use chrono::{TimeZone, Utc};
use vector_core::event::{
    Event, Metric as MetricEvent, MetricKind, MetricTags, MetricValue,
    metric::{Bucket, Quantile, TagValue},
};

use super::common::tag_set_to_any_value;
use super::proto::{
    collector::metrics::v1::ExportMetricsServiceRequest,
    common::v1::{AnyValue, InstrumentationScope, KeyValue},
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

fn push_key_value(attributes: &mut Vec<KeyValue>, key: &str, value: AnyValue) {
    if key.trim().is_empty() {
        return;
    }
    attributes.push(KeyValue {
        key: key.to_string(),
        value: Some(value),
    });
}

pub fn split_metric_tags(tags: &MetricTags) -> (Resource, InstrumentationScope, Vec<KeyValue>) {
    let mut resource_attributes = Vec::new();
    let mut scope_name = String::new();
    let mut scope_version = String::new();
    let mut scope_attributes = Vec::new();
    let mut attributes = Vec::new();

    for (key, tag_set) in tags.iter_sets().filter(|(key, _)| !key.trim().is_empty()) {
        // `scope.name`/`scope.version` are scalar string fields on `InstrumentationScope`,
        // not attributes, so they collapse to a single representative value.
        if key == "scope.name" {
            scope_name = tag_set.as_single().unwrap_or_default().to_string();
            continue;
        } else if key == "scope.version" {
            scope_version = tag_set.as_single().unwrap_or_default().to_string();
            continue;
        }

        let Some(value) = tag_set_to_any_value(tag_set) else {
            continue;
        };

        if let Some(rest) = key.strip_prefix("resource.") {
            push_key_value(&mut resource_attributes, rest, value);
        } else if let Some(rest) = key.strip_prefix("scope.") {
            push_key_value(&mut scope_attributes, rest, value);
        } else {
            push_key_value(&mut attributes, key, value);
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

fn reject_duplicate_values(
    values: impl Iterator<Item = f64>,
    description: &str,
) -> Result<(), vector_common::Error> {
    let mut previous: Option<f64> = None;
    for value in values {
        if previous == Some(value) {
            return Err(format!(
                "{description} must be strictly increasing, but found duplicate value {value}"
            )
            .into());
        }
        previous = Some(value);
    }
    Ok(())
}

fn reject_invalid_sum(
    count: &u64,
    sum: &f64,
    description: &str,
) -> Result<Option<f64>, vector_common::Error> {
    if sum.is_nan() {
        return Err(format!("{description} sum must not be NaN").into());
    }
    if *count == 0 && *sum != 0.0 {
        Err(format!("{description} sum ({sum}) must be zero when count is zero").into())
    } else {
        let sum = if *sum >= 0.0 { Some(*sum) } else { None };
        Ok(sum)
    }
}

struct OTLPDataConverter {
    kind: MetricKind,
    timestamp_ns: u64,
    start_time_ns: u64,
    attrs: Vec<KeyValue>,
    temporality: i32,
}

impl OTLPDataConverter {
    fn new(kind: MetricKind, timestamp_ns: u64, start_time_ns: u64, attrs: Vec<KeyValue>) -> Self {
        let temporality = match kind {
            MetricKind::Incremental => AggregationTemporality::Delta,
            MetricKind::Absolute => AggregationTemporality::Cumulative,
        } as i32;
        Self {
            kind,
            timestamp_ns,
            start_time_ns,
            attrs,
            temporality,
        }
    }

    fn metric_value_to_data(self, value: &MetricValue) -> Result<Data, vector_common::Error> {
        match value {
            MetricValue::Counter { value } => self.counter(value),
            MetricValue::Gauge { value } => self.gauge(value),
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => self.aggregated_histogram(buckets, count, sum),
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => self.aggregated_summary(quantiles, count, sum),
            MetricValue::Set { .. } => {
                Err("OTLP serializer does not support Set (statsd-style) metric values".into())
            }
            MetricValue::Distribution { .. } => Err(
                "OTLP serializer does not support Distribution (un-aggregated) metric values"
                    .into(),
            ),
            MetricValue::Sketch { .. } => {
                Err("OTLP serializer does not support Sketch (DDSketch) metric values".into())
            }
        }
    }
    fn counter(self, value: &f64) -> Result<Data, vector_common::Error> {
        if value.is_nan() {
            return Err("counter value must not be NaN".into());
        }
        match self.kind {
            MetricKind::Absolute => {
                if *value < 0.0 {
                    Err("absolute counter value cannot be negative".into())
                } else {
                    Ok(Data::Sum(Sum {
                        data_points: vec![NumberDataPoint {
                            attributes: self.attrs,
                            start_time_unix_nano: self.start_time_ns,
                            time_unix_nano: self.timestamp_ns,
                            value: Some(NumberDataPointValue::AsDouble(*value)),
                            exemplars: Vec::new(),
                            flags: 0,
                        }],
                        aggregation_temporality: self.temporality,
                        is_monotonic: true,
                    }))
                }
            }
            MetricKind::Incremental => Ok(Data::Sum(Sum {
                data_points: vec![NumberDataPoint {
                    attributes: self.attrs,
                    start_time_unix_nano: self.start_time_ns,
                    time_unix_nano: self.timestamp_ns,
                    value: Some(NumberDataPointValue::AsDouble(*value)),
                    exemplars: Vec::new(),
                    flags: 0,
                }],
                aggregation_temporality: self.temporality,
                is_monotonic: *value >= 0.0,
            })),
        }
    }

    fn gauge(self, value: &f64) -> Result<Data, vector_common::Error> {
        if value.is_nan() {
            return Err("gauge value must not be NaN".into());
        }
        Ok(match self.kind {
            MetricKind::Absolute => Data::Gauge(Gauge {
                data_points: vec![NumberDataPoint {
                    attributes: self.attrs,
                    start_time_unix_nano: 0,
                    time_unix_nano: self.timestamp_ns,
                    value: Some(NumberDataPointValue::AsDouble(*value)),
                    exemplars: Vec::new(),
                    flags: 0,
                }],
            }),
            MetricKind::Incremental => Data::Sum(Sum {
                data_points: vec![NumberDataPoint {
                    attributes: self.attrs,
                    start_time_unix_nano: self.start_time_ns,
                    time_unix_nano: self.timestamp_ns,
                    value: Some(NumberDataPointValue::AsDouble(*value)),
                    exemplars: Vec::new(),
                    flags: 0,
                }],
                aggregation_temporality: self.temporality,
                is_monotonic: false,
            }),
        })
    }

    fn aggregated_histogram(
        self,
        buckets: &[Bucket],
        count: &u64,
        sum: &f64,
    ) -> Result<Data, vector_common::Error> {
        if let Some(bucket) = buckets.iter().find(|bucket| bucket.upper_limit.is_nan()) {
            return Err(format!(
                "histogram bucket upper_limit must not be NaN, got {}",
                bucket.upper_limit
            )
            .into());
        }

        let mut buckets = buckets.to_owned();
        buckets.sort_by(|a, b| a.upper_limit.total_cmp(&b.upper_limit));

        reject_duplicate_values(
            buckets.iter().map(|bucket| bucket.upper_limit),
            "histogram bucket upper_limit",
        )?;

        let sum = reject_invalid_sum(count, sum, "histogram")?;

        // A bucket with a negative upper bound and a nonzero count proves at least one
        // negative event was recorded, even if the aggregate sum is non-negative. OTLP
        // requires sum to be omitted in that case (metrics.proto: sum should only be
        // filled out when measuring non-negative events).
        let has_negative_bucket = buckets
            .iter()
            .any(|bucket| bucket.upper_limit < 0.0 && bucket.count > 0);
        let sum = if has_negative_bucket { None } else { sum };

        let mut bucket_counts: Vec<u64> = buckets.iter().map(|bucket| bucket.count).collect();
        let has_inf_bucket = buckets
            .last()
            .is_some_and(|bucket| bucket.upper_limit == f64::INFINITY);

        let explicit_bounds: Vec<f64> = if has_inf_bucket {
            buckets
                .iter()
                .take(buckets.len() - 1)
                .map(|bucket| bucket.upper_limit)
                .collect()
        } else {
            let bounds = buckets.iter().map(|bucket| bucket.upper_limit).collect();
            let observed: u64 = bucket_counts.iter().sum();
            bucket_counts.push(count.saturating_sub(observed));
            bounds
        };

        let bucket_counts_sum: u64 = bucket_counts.iter().sum();
        if bucket_counts_sum != *count {
            return Err(format!(
                "histogram bucket_counts sum ({bucket_counts_sum}) does not equal count ({count})"
            )
            .into());
        }

        Ok(Data::Histogram(Histogram {
            data_points: vec![HistogramDataPoint {
                attributes: self.attrs,
                start_time_unix_nano: self.start_time_ns,
                time_unix_nano: self.timestamp_ns,
                count: *count,
                sum,
                bucket_counts,
                explicit_bounds,
                exemplars: Vec::new(),
                flags: 0,
                min: None,
                max: None,
            }],
            aggregation_temporality: self.temporality,
        }))
    }

    fn aggregated_summary(
        self,
        quantiles: &[Quantile],
        count: &u64,
        sum: &f64,
    ) -> Result<Data, vector_common::Error> {
        if matches!(self.kind, MetricKind::Incremental) {
            return Err("OTLP serializer does not support Delta summary metric values".into());
        }

        if let Some(quantile) = quantiles
            .iter()
            .find(|q| !(0.0..=1.0).contains(&q.quantile))
        {
            return Err(format!(
                "summary quantile {} is outside the valid range [0.0, 1.0]",
                quantile.quantile
            )
            .into());
        }
        if let Some(quantile) = quantiles.iter().find(|q| q.value.is_nan() || q.value < 0.0) {
            return Err(format!(
                "summary quantile value {} must not be negative or NaN",
                quantile.value
            )
            .into());
        }
        let mut quantiles = quantiles.to_owned();
        quantiles.sort_by(|a, b| a.quantile.total_cmp(&b.quantile));

        reject_duplicate_values(
            quantiles.iter().map(|quantile| quantile.quantile),
            "summary quantile",
        )?;

        let sum =
            reject_invalid_sum(count, sum, "summary")?.ok_or_else(|| -> vector_common::Error {
                "summary sum must not be negative; the OTLP summary sum field cannot be omitted"
                    .into()
            })?;

        let quantile_values = quantiles
            .iter()
            .map(|quantile| ValueAtQuantile {
                quantile: quantile.quantile,
                value: quantile.value,
            })
            .collect();

        Ok(Data::Summary(Summary {
            data_points: vec![SummaryDataPoint {
                attributes: self.attrs,
                start_time_unix_nano: 0,
                time_unix_nano: self.timestamp_ns,
                count: *count,
                sum,
                quantile_values,
                flags: 0,
            }],
        }))
    }
}
fn metric_value_to_data(
    value: &MetricValue,
    kind: MetricKind,
    timestamp_ns: u64,
    start_time_ns: u64,
    attrs: Vec<KeyValue>,
) -> Result<Data, vector_common::Error> {
    OTLPDataConverter::new(kind, timestamp_ns, start_time_ns, attrs).metric_value_to_data(value)
}

pub fn metric_event_to_export_request(
    metric: &MetricEvent,
) -> Result<ExportMetricsServiceRequest, vector_common::Error> {
    let (start_time_ns, timestamp_ns): (u64, u64) = match metric.timestamp() {
        Some(timestamp) => {
            let timestamp_nanos = timestamp.timestamp_nanos_opt().ok_or_else(|| -> vector_common::Error {
	            format!("metric timestamp {timestamp} is outside the range representable as an OTLP nanosecond timestamp").into()
	        })?;
            let timestamp_ns = u64::try_from(timestamp_nanos).map_err(|_| -> vector_common::Error {
	            format!("metric timestamp {timestamp_nanos} is before the Unix epoch and cannot be encoded as an OTLP nanosecond timestamp").into()
	        })?;

            match (metric.kind(), metric.interval_ms()) {
                (MetricKind::Incremental, Some(interval)) => (
                    timestamp_ns,
                    timestamp_ns.saturating_add(u64::from(interval.get()) * 1_000_000),
                ),
                _ => (0, timestamp_ns),
            }
        }
        None => (0, 0),
    };

    let empty_tags = MetricTags::default();
    let tags = metric.tags().unwrap_or(&empty_tags);
    let (resource, scope, attributes) = split_metric_tags(tags);

    let kind = metric.kind();

    let name = match metric.namespace() {
        Some(namespace) => format!("{namespace}.{}", metric.name()),
        None => metric.name().to_string(),
    };
    let data = metric_value_to_data(
        metric.value(),
        kind,
        timestamp_ns,
        start_time_ns,
        attributes,
    )?;

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
    use super::super::common::str_to_key_value;
    use super::*;
    use vector_core::event::MetricValue;

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

        let data = metric_value_to_data(metric.value(), metric.kind(), 1_000, 0, Vec::new())
            .expect("counter should encode");

        match data {
            Data::Sum(sum) => {
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
        let data = metric_value_to_data(metric.value(), metric.kind(), 1, 0, Vec::new()).unwrap();
        match data {
            Data::Sum(sum) => assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            ),
            other => panic!("expected Data::Sum, got {other:?}"),
        }
    }

    #[test]
    fn incremental_interval_sets_delta_start_time() {
        use std::num::NonZeroU32;

        let time_ns = 1_000_000_000; // 1s
        let interval_ns = 10_000_000; // 10ms

        let sum_point = |metric: MetricEvent| {
            let request = metric_event_to_export_request(&metric).expect("counter should encode");
            match request.resource_metrics[0].scope_metrics[0].metrics[0]
                .data
                .clone()
                .unwrap()
            {
                Data::Sum(sum) => sum.data_points.into_iter().next().unwrap(),
                other => panic!("expected Data::Sum, got {other:?}"),
            }
        };

        // Delta (Incremental) with an interval derives the aggregation window end.
        let point = sum_point(
            MetricEvent::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(time_ns)))
            .with_interval_ms(NonZeroU32::new(10)),
        );
        assert_eq!(point.start_time_unix_nano, time_ns as u64);
        assert_eq!(point.time_unix_nano, (time_ns + interval_ns) as u64);

        // Cumulative (Absolute) must NOT derive a start time even if an interval is present.
        let point = sum_point(
            MetricEvent::new(
                "requests",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(time_ns)))
            .with_interval_ms(NonZeroU32::new(10)),
        );
        assert_eq!(point.start_time_unix_nano, 0);

        // Incremental without an interval leaves the start unset.
        let point = sum_point(
            MetricEvent::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(time_ns))),
        );
        assert_eq!(point.start_time_unix_nano, 0);
    }

    #[test]
    fn pre_epoch_timestamp_is_rejected() {
        // A pre-epoch instant yields a negative nanosecond count; it must be rejected rather than
        // wrapping into a far-future unsigned OTLP timestamp.
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(-1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn nan_counter_value_is_rejected() {
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Incremental,
            MetricValue::Counter { value: f64::NAN },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn nan_gauge_value_is_rejected() {
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Absolute,
            MetricValue::Gauge { value: f64::NAN },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn nan_histogram_bucket_bound_is_rejected() {
        // A single NaN bound among several otherwise-valid buckets must still be caught.
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 1.0,
                        count: 1,
                    },
                    Bucket {
                        upper_limit: f64::NAN,
                        count: 1,
                    },
                    Bucket {
                        upper_limit: 5.0,
                        count: 1,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 1,
                    },
                ],
                count: 4,
                sum: 10.0,
            },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn negative_bucket_with_nonzero_count_omits_histogram_sum() {
        // A bucket with a negative upper bound and a nonzero count proves a negative
        // event was recorded, even though the aggregate sum here is positive. OTLP
        // requires sum to be omitted in that case rather than emitted as-is.
        let buckets = vec![
            Bucket {
                upper_limit: -5.0,
                count: 1,
            },
            Bucket {
                upper_limit: 1.0,
                count: 0,
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
                count: 4,
                sum: 10.0,
            },
        );

        let data = metric_value_to_data(metric.value(), metric.kind(), 42, 0, Vec::new()).unwrap();
        match data {
            Data::Histogram(histogram) => {
                let point = histogram.data_points.into_iter().next().unwrap();
                assert_eq!(point.sum, None);
            }
            other => panic!("expected Data::Histogram, got {other:?}"),
        }
    }

    #[test]
    fn negative_bucket_with_zero_count_keeps_histogram_sum() {
        // A negative bound with a zero count does not prove any negative event was
        // recorded, so the aggregate sum must still be emitted.
        let buckets = vec![
            Bucket {
                upper_limit: -5.0,
                count: 0,
            },
            Bucket {
                upper_limit: 1.0,
                count: 1,
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
                count: 4,
                sum: 10.0,
            },
        );

        let data = metric_value_to_data(metric.value(), metric.kind(), 42, 0, Vec::new()).unwrap();
        match data {
            Data::Histogram(histogram) => {
                let point = histogram.data_points.into_iter().next().unwrap();
                assert_eq!(point.sum, Some(10.0));
            }
            other => panic!("expected Data::Histogram, got {other:?}"),
        }
    }

    #[test]
    fn nan_histogram_sum_is_rejected() {
        // A NaN sum must be rejected even when multiple valid buckets are present.
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 1.0,
                        count: 1,
                    },
                    Bucket {
                        upper_limit: 5.0,
                        count: 1,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 1,
                    },
                ],
                count: 3,
                sum: f64::NAN,
            },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn nan_summary_sum_is_rejected() {
        // A NaN sum must be rejected even when multiple valid quantiles are present.
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 1.0,
                    },
                    Quantile {
                        quantile: 0.9,
                        value: 2.0,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 3.0,
                    },
                ],
                count: 3,
                sum: f64::NAN,
            },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn negative_summary_sum_is_rejected() {
        // Unlike histogram's optional sum, OTLP summary's sum field cannot be omitted,
        // so a negative sum must be rejected
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 1.0,
                    },
                    Quantile {
                        quantile: 0.9,
                        value: 2.0,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 3.0,
                    },
                ],
                count: 3,
                sum: -1.0,
            },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn nan_summary_quantile_value_is_rejected() {
        // A single NaN quantile value among several otherwise-valid quantiles must still be caught.
        let metric = MetricEvent::new(
            "latency",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 1.0,
                    },
                    Quantile {
                        quantile: 0.9,
                        value: f64::NAN,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 3.0,
                    },
                ],
                count: 3,
                sum: 6.0,
            },
        )
        .with_timestamp(Some(Utc.timestamp_nanos(1_000)));

        assert!(metric_event_to_export_request(&metric).is_err());
    }

    #[test]
    fn namespace_is_prefixed_onto_metric_name() {
        // Namespaced metrics must keep distinct OTLP identities instead of colliding on the bare
        // name. The namespace is joined with `.` (OTLP's namespace separator).
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        )
        .with_namespace(Some("vector"))
        .with_timestamp(Some(Utc::now()));

        let request = metric_event_to_export_request(&metric).expect("should encode");
        assert_eq!(
            request.resource_metrics[0].scope_metrics[0].metrics[0].name,
            "vector.requests"
        );

        // No namespace leaves the bare name untouched.
        let metric = MetricEvent::new(
            "requests",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        )
        .with_timestamp(Some(Utc::now()));
        let request = metric_event_to_export_request(&metric).expect("should encode");
        assert_eq!(
            request.resource_metrics[0].scope_metrics[0].metrics[0].name,
            "requests"
        );
    }

    #[test]
    fn gauge_to_otlp_gauge() {
        let attrs = vec![str_to_key_value("host", &TagValue::from("localhost"))];
        let metric = MetricEvent::new(
            "cpu",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 12.5 },
        );

        let data =
            metric_value_to_data(metric.value(), metric.kind(), 5, 0, attrs.clone()).unwrap();
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

        let data = metric_value_to_data(metric.value(), metric.kind(), 42, 0, Vec::new()).unwrap();
        match data {
            Data::Histogram(histogram) => {
                let point = histogram.data_points.into_iter().next().unwrap();
                assert_eq!(point.bucket_counts, vec![1, 2, 3]);
                // Explicit +Inf bucket: drop only its bound, keep every count. N counts, N-1 bounds.
                assert_eq!(point.explicit_bounds, vec![1.0, 2.0]);
                assert_eq!(point.count, 6);
                assert_eq!(point.sum, Some(10.0));
            }
            other => panic!("expected Data::Histogram, got {other:?}"),
        }
    }

    #[test]
    fn prometheus_histogram_appends_overflow_bucket() {
        // Prometheus-derived histograms carry only finite bounds; `count` (6) exceeds the sum of
        // bucket counts (1 + 2 = 3), the extra 3 being observations above the last bound.
        let buckets = vec![
            Bucket {
                upper_limit: 1.0,
                count: 1,
            },
            Bucket {
                upper_limit: 2.0,
                count: 2,
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

        let data = metric_value_to_data(metric.value(), metric.kind(), 42, 0, Vec::new()).unwrap();
        match data {
            Data::Histogram(histogram) => {
                let point = histogram.data_points.into_iter().next().unwrap();
                // Every finite bound is kept; the overflow (6 - 3 = 3) becomes the +Inf bucket.
                assert_eq!(point.explicit_bounds, vec![1.0, 2.0]);
                assert_eq!(point.bucket_counts, vec![1, 2, 3]);
                // OTLP invariant: bounds.len() + 1 == counts.len(), and count == sum(counts).
                assert_eq!(point.explicit_bounds.len() + 1, point.bucket_counts.len());
                assert_eq!(point.count, point.bucket_counts.iter().sum::<u64>());
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

        let data = metric_value_to_data(metric.value(), metric.kind(), 1, 0, Vec::new()).unwrap();
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
}

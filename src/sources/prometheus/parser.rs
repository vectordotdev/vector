#[cfg(feature = "sources-prometheus-remote-write")]
use super::remote_write::MetadataConflictStrategy;
use chrono::{DateTime, TimeZone, Utc};
use vector_lib::prometheus::parser::{GroupKind, MetricGroup, ParserError};
#[cfg(feature = "sources-prometheus-remote-write")]
use vector_lib::prometheus::parser::{
    MetadataConflictStrategy as ParserMetadataConflictStrategy, proto,
};

use crate::event::{
    Event,
    metric::{Bucket, Metric, MetricKind, MetricTags, MetricValue, Quantile},
};

fn utc_timestamp(timestamp: Option<i64>, default: DateTime<Utc>) -> DateTime<Utc> {
    timestamp
        .and_then(|timestamp| {
            Utc.timestamp_opt(timestamp / 1000, (timestamp % 1000) as u32 * 1000000)
                .latest()
        })
        .unwrap_or(default)
}

#[cfg(any(test, feature = "sources-prometheus-scrape"))]
pub(super) fn parse_text(packet: &str) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_text(packet)
        .map(|group| reparse_groups(group, vec![], false, false))
}

#[cfg(any(test, feature = "sources-prometheus-pushgateway"))]
pub(super) fn parse_text_with_overrides(
    packet: &str,
    tag_overrides: impl IntoIterator<Item = (String, String)> + Clone,
    aggregate_metrics: bool,
) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_text(packet)
        .map(|group| reparse_groups(group, tag_overrides, aggregate_metrics, false))
}

#[cfg(test)]
fn parse_text_with_nan_filtering(packet: &str) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_text(packet)
        .map(|group| reparse_groups(group, vec![], false, true))
}

#[cfg(feature = "sources-prometheus-remote-write")]
pub(super) fn parse_request(
    request: proto::WriteRequest,
    metadata_conflict_strategy: MetadataConflictStrategy,
    skip_nan_values: bool,
) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_request(request, metadata_conflict_strategy.into())
        .map(|group| reparse_groups(group, vec![], false, skip_nan_values))
}

fn reparse_groups(
    groups: Vec<MetricGroup>,
    tag_overrides: impl IntoIterator<Item = (String, String)> + Clone,
    aggregate_metrics: bool,
    skip_nan_values: bool,
) -> Vec<Event> {
    let mut result = Vec::new();
    let start = Utc::now();

    let metric_kind = if aggregate_metrics {
        MetricKind::Incremental
    } else {
        MetricKind::Absolute
    };

    for group in groups {
        match group.metrics {
            GroupKind::Counter(metrics) => {
                for (key, metric) in metrics {
                    if skip_nan_values && metric.value.is_nan() {
                        continue;
                    }

                    let tags = combine_tags(key.labels, tag_overrides.clone());

                    let counter = Metric::new(
                        group.name.clone(),
                        metric_kind,
                        MetricValue::Counter {
                            value: metric.value,
                        },
                    )
                    .with_timestamp(Some(utc_timestamp(key.timestamp, start)))
                    .with_tags(tags.as_option());

                    result.push(counter.into());
                }
            }
            GroupKind::Gauge(metrics) | GroupKind::Untyped(metrics) => {
                for (key, metric) in metrics {
                    if skip_nan_values && metric.value.is_nan() {
                        continue;
                    }

                    let tags = combine_tags(key.labels, tag_overrides.clone());

                    let gauge = Metric::new(
                        group.name.clone(),
                        // Gauges are always absolute: aggregating them makes no sense
                        MetricKind::Absolute,
                        MetricValue::Gauge {
                            value: metric.value,
                        },
                    )
                    .with_timestamp(Some(utc_timestamp(key.timestamp, start)))
                    .with_tags(tags.as_option());

                    result.push(gauge.into());
                }
            }
            GroupKind::Histogram(metrics) => {
                for (key, metric) in metrics {
                    if skip_nan_values
                        && (metric.sum.is_nan() || metric.buckets.iter().any(|b| b.bucket.is_nan()))
                    {
                        continue;
                    }

                    let tags = combine_tags(key.labels, tag_overrides.clone());

                    let mut buckets = metric.buckets;
                    buckets.sort_unstable_by(|a, b| {
                        a.bucket.total_cmp(&b.bucket).then(a.count.cmp(&b.count))
                    });
                    for i in (1..buckets.len()).rev() {
                        buckets[i].count = buckets[i].count.saturating_sub(buckets[i - 1].count);
                    }
                    let drop_last = buckets
                        .last()
                        .is_some_and(|bucket| bucket.bucket == f64::INFINITY);
                    if drop_last {
                        buckets.pop();
                    }

                    result.push(
                        Metric::new(
                            group.name.clone(),
                            metric_kind,
                            MetricValue::AggregatedHistogram {
                                buckets: buckets
                                    .into_iter()
                                    .map(|b| Bucket {
                                        upper_limit: b.bucket,
                                        count: b.count,
                                    })
                                    .collect(),
                                count: metric.count,
                                sum: metric.sum,
                            },
                        )
                        .with_timestamp(Some(utc_timestamp(key.timestamp, start)))
                        .with_tags(tags.as_option())
                        .into(),
                    );
                }
            }
            GroupKind::Summary(metrics) => {
                for (key, metric) in metrics {
                    if skip_nan_values
                        && (metric.sum.is_nan()
                            || metric
                                .quantiles
                                .iter()
                                .any(|q| q.quantile.is_nan() || q.value.is_nan()))
                    {
                        continue;
                    }

                    let tags = combine_tags(key.labels, tag_overrides.clone());

                    result.push(
                        Metric::new(
                            group.name.clone(),
                            // Summaries are always absolute: aggregating them makes no sense
                            MetricKind::Absolute,
                            MetricValue::AggregatedSummary {
                                quantiles: metric
                                    .quantiles
                                    .into_iter()
                                    .map(|q| Quantile {
                                        quantile: q.quantile,
                                        value: q.value,
                                    })
                                    .collect(),
                                count: metric.count,
                                sum: metric.sum,
                            },
                        )
                        .with_timestamp(Some(utc_timestamp(key.timestamp, start)))
                        .with_tags(tags.as_option())
                        .into(),
                    );
                }
            }
        }
    }

    result
}

#[cfg(feature = "sources-prometheus-remote-write")]
impl From<MetadataConflictStrategy> for ParserMetadataConflictStrategy {
    fn from(strategy: MetadataConflictStrategy) -> Self {
        match strategy {
            MetadataConflictStrategy::Ignore => Self::Ignore,
            MetadataConflictStrategy::Reject => Self::Reject,
        }
    }
}

fn combine_tags(
    base_tags: impl Into<MetricTags>,
    tag_overrides: impl IntoIterator<Item = (String, String)>,
) -> MetricTags {
    let mut tags = base_tags.into();
    for (k, v) in tag_overrides.into_iter() {
        tags.replace(k, v);
    }

    tags
}

#[cfg(test)]
mod test;

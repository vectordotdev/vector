use std::cmp::Ordering;

use chrono::{DateTime, TimeZone, Utc};
#[cfg(feature = "sources-prometheus-remote-write")]
use vector_lib::prometheus::parser::proto;
use vector_lib::prometheus::parser::{GroupKind, MetricGroup, ParserError};

use crate::event::{
    metric::{Bucket, Metric, MetricKind, MetricTags, MetricValue, Quantile},
    Event,
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
        .map(|group| reparse_groups(group, vec![], false))
}

#[cfg(any(test, feature = "sources-prometheus-pushgateway"))]
pub(super) fn parse_text_with_overrides(
    packet: &str,
    tag_overrides: impl IntoIterator<Item = (String, String)> + Clone,
    aggregate_metrics: bool,
) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_text(packet)
        .map(|group| reparse_groups(group, tag_overrides, aggregate_metrics))
}

#[cfg(feature = "sources-prometheus-remote-write")]
pub(super) fn parse_request(request: proto::WriteRequest) -> Result<Vec<Event>, ParserError> {
    vector_lib::prometheus::parser::parse_request(request)
        .map(|group| reparse_groups(group, vec![], false))
}

fn reparse_groups(
    groups: Vec<MetricGroup>,
    tag_overrides: impl IntoIterator<Item = (String, String)> + Clone,
    aggregate_metrics: bool,
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
                    let tags = combine_tags(key.labels, tag_overrides.clone());

                    let mut buckets = metric.buckets;
                    buckets.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                    for i in (1..buckets.len()).rev() {
                        buckets[i].count = buckets[i].count.saturating_sub(buckets[i - 1].count);
                    }
                    let drop_last = buckets
                        .last()
                        .map_or(false, |bucket| bucket.bucket == f64::INFINITY);
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
mod test {
    use chrono::{TimeZone, Timelike, Utc};
    use once_cell::sync::Lazy;
    use similar_asserts::assert_eq;
    use vector_lib::assert_event_data_eq;
    use vector_lib::metric_tags;

    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue};

    static TIMESTAMP: Lazy<DateTime<Utc>> = Lazy::new(|| {
        Utc.with_ymd_and_hms(2021, 2, 4, 4, 5, 6)
            .single()
            .and_then(|t| t.with_nanosecond(789 * 1_000_000))
            .expect("invalid timestamp")
    });

    fn events_to_metrics(
        events: Result<Vec<Event>, ParserError>,
    ) -> Result<Vec<Metric>, ParserError> {
        events.map(|events| events.into_iter().map(Event::into_metric).collect())
    }

    #[test]
    fn adds_timestamp_if_missing() {
        let now = Utc::now();
        let exp = r##"
            # HELP counter Some counter
            # TYPE count counter
            http_requests_total 1027
            "##;
        let result = events_to_metrics(parse_text(exp)).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].timestamp().unwrap() >= now);
    }

    #[test]
    fn test_counter() {
        let exp = r##"
            # HELP uptime A counter
            # TYPE uptime counter
            uptime 123.0 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "uptime",
                MetricKind::Absolute,
                MetricValue::Counter { value: 123.0 },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_counter_empty() {
        let exp = r##"
            # HELP hidden A counter
            # TYPE hidden counter
            "##;

        assert_event_data_eq!(events_to_metrics(parse_text(exp)), Ok(vec![]));
    }

    #[test]
    fn test_counter_nan() {
        let exp = r#"
            # TYPE name counter
            name{labelname="val1",basename="basevalue"} NaN
            "#;

        match events_to_metrics(parse_text(exp)).unwrap()[0].value() {
            MetricValue::Counter { value } => {
                assert!(value.is_nan());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_counter_weird() {
        let exp = r#"
            # A normal comment.
            #
            # TYPE name counter
            name {labelname="val2",basename="base\"v\\al\nue"} 0.23 1612411506789
            # HELP name two-line\n doc  str\\ing
            # HELP  name2  	doc str"ing 2
            #    TYPE    name2 counter
            name2{labelname="val2"	,basename   =   "basevalue2"		} +Inf 1612411506789
            name2{ labelname = "val1" , }-Inf 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "name",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.23 },
                )
                .with_tags(Some(metric_tags!(
                    "labelname" => "val2",
                    "basename" => "base\"v\\al\nue",
                )))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "name2",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: f64::INFINITY
                    },
                )
                .with_tags(Some(metric_tags!(
                    "labelname" => "val2",
                    "basename" => "basevalue2",
                )))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "name2",
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: f64::NEG_INFINITY
                    },
                )
                .with_tags(Some(metric_tags!("labelname" => "val1")))
                .with_timestamp(Some(*TIMESTAMP)),
            ]),
        );
    }

    #[test]
    fn test_counter_tags_and_timestamp() {
        let exp = r#"
            # HELP http_requests_total The total number of HTTP requests.
            # TYPE http_requests_total counter
            http_requests_total{method="post",code="200"} 1027 1395066363000
            http_requests_total{method="post",code="400"}    3 1395066363000
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "http_requests_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1027.0 },
                )
                .with_timestamp(Utc.timestamp_opt(1395066363, 0).latest())
                .with_tags(Some(metric_tags!(
                    "method" => "post",
                    "code" => "200",
                ))),
                Metric::new(
                    "http_requests_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 3.0 },
                )
                .with_timestamp(Utc.timestamp_opt(1395066363, 0).latest())
                .with_tags(Some(metric_tags!(
                    "method" => "post",
                    "code" => "400"
                )))
            ]),
        );
    }

    #[test]
    fn test_gauge() {
        let exp = r##"
            # HELP latency A gauge
            # TYPE latency gauge
            latency 123.0 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "latency",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.0 },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_gauge_minimalistic() {
        let exp = r##"
            metric_without_timestamp_and_labels 12.47 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "metric_without_timestamp_and_labels",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 12.47 },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_gauge_empty_labels() {
        let exp = r##"
            no_labels{} 3 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "no_labels",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 3.0 },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_gauge_minimalistic_escaped() {
        let exp = r#"
            msdos_file_access_time_seconds{path="C:\\DIR\\FILE.TXT",error="Cannot find file:\n\"FILE.TXT\""} 1.458255915e9 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "msdos_file_access_time_seconds",
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: 1458255915.0
                },
            )
            .with_tags(Some(metric_tags!(
                "path" => "C:\\DIR\\FILE.TXT",
                "error" => "Cannot find file:\n\"FILE.TXT\"",
            )))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_tag_value_contain_bracket() {
        let exp = r#"
            # HELP name counter
            # TYPE name counter
            name{tag="}"} 0 1612411506789
            "#;
        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_tags(Some(metric_tags! { "tag" => "}" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_parse_tag_value_contain_comma() {
        let exp = r#"
            # HELP name counter
            # TYPE name counter
            name{tag="a,b"} 0 1612411506789
            "#;
        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_tags(Some(metric_tags! { "tag" => "a,b" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_parse_tag_escaping() {
        let exp = r#"
            # HELP name counter
            # TYPE name counter
            name{tag="\\n"} 0 1612411506789
            "#;
        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_tags(Some(metric_tags! { "tag" => "\\n" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_parse_tag_dont_trim_value() {
        let exp = r#"
            # HELP name counter
            # TYPE name counter
            name{tag=" * "} 0 1612411506789
            "#;
        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "name",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_tags(Some(metric_tags! { "tag" => " * " }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_parse_tag_value_containing_equals() {
        let exp = r#"
            telemetry_scrape_size_bytes_count{registry="default",content_type="text/plain; version=0.0.4"} 1890 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "telemetry_scrape_size_bytes_count",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1890.0 },
            )
            .with_tags(Some(metric_tags!( "registry" => "default",
                    "content_type" => "text/plain; version=0.0.4" )))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_parse_tag_error_no_value() {
        let exp = r#"
            telemetry_scrape_size_bytes_count{registry="default",content_type} 1890 1612411506789
            "#;

        assert!(events_to_metrics(parse_text(exp)).is_err());
    }

    #[test]
    fn test_parse_tag_error_equals_empty_value() {
        let exp = r#"
            telemetry_scrape_size_bytes_count{registry="default",content_type=} 1890 1612411506789
            "#;

        assert!(events_to_metrics(parse_text(exp)).is_err());
    }

    #[test]
    fn test_gauge_weird_timestamp() {
        let exp = r#"
            something_weird{problem="division by zero"} +Inf -3982045000
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "something_weird",
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: f64::INFINITY
                },
            )
            .with_timestamp(Utc.timestamp_opt(-3982045, 0).latest())
            .with_tags(Some(
                metric_tags!("problem" => "division by zero")
            ))]),
        );
    }

    #[test]
    fn test_gauge_tabs() {
        let exp = r#"
            # TYPE	latency	gauge
            latency{env="production"}	1.0		1395066363000
            latency{env="testing"}		2.0		1395066363000
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "latency",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                )
                .with_timestamp(Utc.timestamp_opt(1395066363, 0).latest())
                .with_tags(Some(metric_tags!("env" => "production"))),
                Metric::new(
                    "latency",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
                .with_timestamp(Utc.timestamp_opt(1395066363, 0).latest())
                .with_tags(Some(metric_tags!("env" => "testing")))
            ]),
        );
    }

    #[test]
    fn test_mixed() {
        let exp = r##"
            # TYPE uptime counter
            uptime 123.0 1612411506789
            # TYPE temperature gauge
            temperature -1.5 1612411506789
            # TYPE launch_count counter
            launch_count 10.0 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "uptime",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 123.0 },
                )
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "temperature",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: -1.5 },
                )
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "launch_count",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 10.0 },
                )
                .with_timestamp(Some(*TIMESTAMP))
            ]),
        );
    }

    #[test]
    fn test_no_value() {
        let exp = r#"
            # TYPE latency counter
            latency{env="production"}
            "#;

        assert!(events_to_metrics(parse_text(exp)).is_err());
    }

    #[test]
    fn test_no_name() {
        let exp = r##"
            # TYPE uptime counter
            123.0 1612411506789
            "##;

        assert!(events_to_metrics(parse_text(exp)).is_err());
    }

    #[test]
    fn test_mixed_and_loosely_typed() {
        let exp = r##"
            # TYPE uptime counter
            uptime 123.0 1612411506789
            last_downtime 4.0 1612411506789
            # TYPE temperature gauge
            temperature -1.5 1612411506789
            temperature_7_days_average 0.1 1612411506789
            "##;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "uptime",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 123.0 },
                )
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "last_downtime",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 4.0 },
                )
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "temperature",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: -1.5 },
                )
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "temperature_7_days_average",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.1 },
                )
                .with_timestamp(Some(*TIMESTAMP))
            ]),
        );
    }

    #[test]
    fn test_histogram() {
        let exp = r#"
            # HELP http_request_duration_seconds A histogram of the request duration.
            # TYPE http_request_duration_seconds histogram
            http_request_duration_seconds_bucket{le="0.05"} 24054 1612411506789
            http_request_duration_seconds_bucket{le="0.1"} 33444 1612411506789
            http_request_duration_seconds_bucket{le="0.2"} 100392 1612411506789
            http_request_duration_seconds_bucket{le="0.5"} 129389 1612411506789
            http_request_duration_seconds_bucket{le="1"} 133988 1612411506789
            http_request_duration_seconds_bucket{le="+Inf"} 144320 1612411506789
            http_request_duration_seconds_sum 53423 1612411506789
            http_request_duration_seconds_count 144320 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "http_request_duration_seconds",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![
                        0.05 => 24054, 0.1 => 9390, 0.2 => 66948, 0.5 => 28997, 1.0 => 4599
                    ],
                    count: 144320,
                    sum: 53423.0,
                },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_histogram_out_of_order() {
        let exp = r#"
            # HELP duration A histogram of the request duration.
            # TYPE duration histogram
            duration_bucket{le="+Inf"} 144320 1612411506789
            duration_bucket{le="1"} 133988 1612411506789
            duration_sum 53423 1612411506789
            duration_count 144320 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "duration",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![1.0 => 133988],
                    count: 144320,
                    sum: 53423.0,
                },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_histogram_backward_values() {
        let exp = r#"
            # HELP duration A histogram of the request duration.
            # TYPE duration histogram
            duration_bucket{le="1"} 2000 1612411506789
            duration_bucket{le="10"} 1000 1612411506789
            duration_bucket{le="+Inf"} 2000 1612411506789
            duration_sum 2000 1612411506789
            duration_count 2000 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![Metric::new(
                "duration",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vector_lib::buckets![1.0 => 2000, 10.0 => 0],
                    count: 2000,
                    sum: 2000.0,
                },
            )
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_histogram_with_labels() {
        let exp = r#"
            # HELP gitlab_runner_job_duration_seconds Histogram of job durations
            # TYPE gitlab_runner_job_duration_seconds histogram
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="30"} 327 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="60"} 474 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="300"} 535 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="600"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="1800"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="3600"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="7200"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="10800"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="18000"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="36000"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="+Inf"} 536 1612411506789
            gitlab_runner_job_duration_seconds_sum{runner="z"} 19690.129384881966 1612411506789
            gitlab_runner_job_duration_seconds_count{runner="z"} 536 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="30"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="60"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="300"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="600"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="1800"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="3600"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="7200"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="10800"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="18000"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="36000"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="+Inf"} 1 1612411506789
            gitlab_runner_job_duration_seconds_sum{runner="x"} 28.975436316 1612411506789
            gitlab_runner_job_duration_seconds_count{runner="x"} 1 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="30"} 285 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="60"} 1165 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="300"} 3071 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="600"} 3151 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="1800"} 3252 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="3600"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="7200"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="10800"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="18000"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="36000"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="+Inf"} 3255 1612411506789
            gitlab_runner_job_duration_seconds_sum{runner="y"} 381111.7498891335 1612411506789
            gitlab_runner_job_duration_seconds_count{runner="y"} 3255 1612411506789
        "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "gitlab_runner_job_duration_seconds", MetricKind::Absolute, MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            30.0 => 327,
                            60.0 => 147,
                            300.0 => 61,
                            600.0 => 1,
                            1800.0 => 0,
                            3600.0 => 0,
                            7200.0 => 0,
                            10800.0 => 0,
                            18000.0 => 0,
                            36000.0 => 0
                        ],
                        count: 536,
                        sum: 19690.129384881966,
                    },
                )
                    .with_tags(Some(metric_tags!("runner" => "z")))
                    .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "gitlab_runner_job_duration_seconds", MetricKind::Absolute, MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            30.0 => 1,
                            60.0 => 0,
                            300.0 => 0,
                            600.0 => 0,
                            1800.0 => 0,
                            3600.0 => 0,
                            7200.0 => 0,
                            10800.0 => 0,
                            18000.0 => 0,
                            36000.0 => 0
                        ],
                        count: 1,
                        sum: 28.975436316,
                    },
                )
                    .with_tags(Some(metric_tags!("runner" => "x")))
                    .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "gitlab_runner_job_duration_seconds", MetricKind::Absolute, MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            30.0 => 285, 60.0 => 880, 300.0 => 1906, 600.0 => 80, 1800.0 => 101, 3600.0 => 3,
                            7200.0 => 0, 10800.0 => 0, 18000.0 => 0, 36000.0 => 0
                        ],
                        count: 3255,
                        sum: 381111.7498891335,
                    },
                )
                    .with_tags(Some(metric_tags!("runner" => "y")))
                    .with_timestamp(Some(*TIMESTAMP))
            ]),
        );
    }

    #[test]
    fn test_summary() {
        let exp = r#"
            # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
            # TYPE rpc_duration_seconds summary
            rpc_duration_seconds{service="a",quantile="0.01"} 3102 1612411506789
            rpc_duration_seconds{service="a",quantile="0.05"} 3272 1612411506789
            rpc_duration_seconds{service="a",quantile="0.5"} 4773 1612411506789
            rpc_duration_seconds{service="a",quantile="0.9"} 9001 1612411506789
            rpc_duration_seconds{service="a",quantile="0.99"} 76656 1612411506789
            rpc_duration_seconds_sum{service="a"} 1.7560473e+07 1612411506789
            rpc_duration_seconds_count{service="a"} 2693 1612411506789
            # HELP go_gc_duration_seconds A summary of the GC invocation durations.
            # TYPE go_gc_duration_seconds summary
            go_gc_duration_seconds{quantile="0"} 0.009460965 1612411506789
            go_gc_duration_seconds{quantile="0.25"} 0.009793382 1612411506789
            go_gc_duration_seconds{quantile="0.5"} 0.009870205 1612411506789
            go_gc_duration_seconds{quantile="0.75"} 0.01001838 1612411506789
            go_gc_duration_seconds{quantile="1"} 0.018827136 1612411506789
            go_gc_duration_seconds_sum 4668.551713715 1612411506789
            go_gc_duration_seconds_count 602767 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text(exp)),
            Ok(vec![
                Metric::new(
                    "rpc_duration_seconds",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: vector_lib::quantiles![
                            0.01 => 3102.0,
                            0.05 => 3272.0,
                            0.5 => 4773.0,
                            0.9 => 9001.0,
                            0.99 => 76656.0
                        ],
                        count: 2693,
                        sum: 1.7560473e+07,
                    },
                )
                .with_tags(Some(metric_tags!("service" => "a")))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "go_gc_duration_seconds",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: vector_lib::quantiles![
                            0.0 => 0.009460965,
                            0.25 => 0.009793382,
                            0.5 => 0.009870205,
                            0.75 => 0.01001838,
                            1.0 => 0.018827136
                        ],
                        count: 602767,
                        sum: 4668.551713715,
                    },
                )
                .with_timestamp(Some(*TIMESTAMP)),
            ]),
        );
    }

    // https://github.com/vectordotdev/vector/issues/3276
    #[test]
    fn test_nginx() {
        let exp = r#"
            # HELP nginx_server_bytes request/response bytes
            # TYPE nginx_server_bytes counter
            nginx_server_bytes{direction="in",host="*"} 263719
            nginx_server_bytes{direction="in",host="_"} 255061
            nginx_server_bytes{direction="in",host="nginx-vts-status"} 8658
            nginx_server_bytes{direction="out",host="*"} 944199
            nginx_server_bytes{direction="out",host="_"} 360775
            nginx_server_bytes{direction="out",host="nginx-vts-status"} 583424
            # HELP nginx_server_cache cache counter
            # TYPE nginx_server_cache counter
            nginx_server_cache{host="*",status="bypass"} 0
            nginx_server_cache{host="*",status="expired"} 0
            nginx_server_cache{host="*",status="hit"} 0
            nginx_server_cache{host="*",status="miss"} 0
            nginx_server_cache{host="*",status="revalidated"} 0
            nginx_server_cache{host="*",status="scarce"} 0
            "#;

        let now = Utc::now();
        let result = events_to_metrics(parse_text(exp)).expect("Parsing failed");
        // Reset all the timestamps for comparison
        let result: Vec<_> = result
            .into_iter()
            .map(|metric| {
                assert!(metric.timestamp().expect("Missing timestamp") >= now);
                metric.with_timestamp(Some(*TIMESTAMP))
            })
            .collect();

        assert_event_data_eq!(
            result,
            vec![
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 263719.0 },
                )
                .with_tags(Some(metric_tags! { "direction" => "in", "host" => "*" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 255061.0 },
                )
                .with_tags(Some(metric_tags! { "direction" => "in", "host" => "_" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 8658.0 },
                )
                .with_tags(Some(
                    metric_tags! { "direction" => "in", "host" => "nginx-vts-status" }
                ))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 944199.0 },
                )
                .with_tags(Some(metric_tags! { "direction" => "out", "host" => "*" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 360775.0 },
                )
                .with_tags(Some(metric_tags! { "direction" => "out", "host" => "_" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_bytes",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 583424.0 },
                )
                .with_tags(Some(
                    metric_tags! { "direction" => "out", "host" => "nginx-vts-status" }
                ))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(metric_tags! { "host" => "*", "status" => "bypass" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(metric_tags! { "host" => "*", "status" => "expired" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(metric_tags! { "host" => "*", "status" => "hit" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(metric_tags! { "host" => "*", "status" => "miss" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(
                    metric_tags! { "host" => "*", "status" => "revalidated" }
                ))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "nginx_server_cache",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
                .with_tags(Some(metric_tags! { "host" => "*", "status" => "scarce" }))
                .with_timestamp(Some(*TIMESTAMP))
            ]
        );
    }

    #[test]
    fn test_overrides_nothing_overwritten() {
        let exp = r#"
            # TYPE jobs_total counter
            # HELP jobs_total Total number of jobs
            jobs_total{type="a"} 1.0 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text_with_overrides(exp, vec![], false)),
            Ok(vec![Metric::new(
                "jobs_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(metric_tags! { "type" => "a" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_overrides_label_overwritten() {
        let exp = r#"
            # TYPE jobs_total counter
            # HELP jobs_total Total number of jobs
            jobs_total{type="a"} 1.0 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text_with_overrides(
                exp,
                vec![("type".to_owned(), "b".to_owned())],
                false
            )),
            Ok(vec![Metric::new(
                "jobs_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(metric_tags! { "type" => "b" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    // This matches the behaviour of the real Prometheus Pushgateway, which I
    // tested manually.
    #[test]
    fn test_overrides_last_value_preferred() {
        let exp = r#"
            # TYPE jobs_total counter
            # HELP jobs_total Total number of jobs
            jobs_total{type="a"} 1.0 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text_with_overrides(
                exp,
                vec![
                    ("type".to_owned(), "b".to_owned()),
                    ("type".to_owned(), "c".to_owned())
                ],
                false
            )),
            Ok(vec![Metric::new(
                "jobs_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            )
            .with_tags(Some(metric_tags! { "type" => "c" }))
            .with_timestamp(Some(*TIMESTAMP))]),
        );
    }

    #[test]
    fn test_aggregation_enabled_only_aggregates_counter_and_histogram() {
        let exp = r#"
            # TYPE jobs_total counter
            # HELP jobs_total Total number of jobs
            jobs_total{type="a"} 1.0 1612411506789
            # TYPE jobs_current gauge
            # HELP jobs_current Current number of jobs
            jobs_current{type="a"} 5.0 1612411506789
            # TYPE jobs_distribution histogram
            # HELP jobs_distribution Distribution of jobs
            jobs_distribution_bucket{type="a",le="1"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="2.5"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="5"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="10"} 1.0 1612411506789
            jobs_distribution_bucket{type="a",le="+Inf"} 1.0 1612411506789
            jobs_distribution_sum{type="a"} 8.0 1612411506789
            jobs_distribution_count{type="a"} 1.0 1612411506789
            # TYPE jobs_summary summary
            # HELP jobs_summary Summary of jobs
            jobs_summary_sum{type="a"} 8.0 1612411506789
            jobs_summary_count{type="a"} 1.0 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text_with_overrides(exp, vec![], true)),
            Ok(vec![
                Metric::new(
                    "jobs_total",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_current",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 5.0 },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_distribution",
                    MetricKind::Incremental,
                    MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            1.0 => 0, 2.5 => 0, 5.0 => 0, 10.0 => 1
                        ],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_summary",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: vector_lib::quantiles![],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
            ]),
        );
    }

    #[test]
    fn test_aggregation_disabled_all_absolute() {
        let exp = r#"
            # TYPE jobs_total counter
            # HELP jobs_total Total number of jobs
            jobs_total{type="a"} 1.0 1612411506789
            # TYPE jobs_current gauge
            # HELP jobs_current Current number of jobs
            jobs_current{type="a"} 5.0 1612411506789
            # TYPE jobs_distribution histogram
            # HELP jobs_distribution Distribution of jobs
            jobs_distribution_bucket{type="a",le="1"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="2.5"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="5"} 0.0 1612411506789
            jobs_distribution_bucket{type="a",le="10"} 1.0 1612411506789
            jobs_distribution_bucket{type="a",le="+Inf"} 1.0 1612411506789
            jobs_distribution_sum{type="a"} 8.0 1612411506789
            jobs_distribution_count{type="a"} 1.0 1612411506789
            # TYPE jobs_summary summary
            # HELP jobs_summary Summary of jobs
            jobs_summary_sum{type="a"} 8.0 1612411506789
            jobs_summary_count{type="a"} 1.0 1612411506789
            "#;

        assert_event_data_eq!(
            events_to_metrics(parse_text_with_overrides(exp, vec![], false)),
            Ok(vec![
                Metric::new(
                    "jobs_total",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_current",
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 5.0 },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_distribution",
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: vector_lib::buckets![
                            1.0 => 0, 2.5 => 0, 5.0 => 0, 10.0 => 1
                        ],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
                Metric::new(
                    "jobs_summary",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: vector_lib::quantiles![],
                        count: 1,
                        sum: 8.0,
                    },
                )
                .with_tags(Some(metric_tags! { "type" => "a" }))
                .with_timestamp(Some(*TIMESTAMP)),
            ]),
        );
    }
}

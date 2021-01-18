use crate::event::metric::{Bucket, Metric, MetricKind, MetricValue, Quantile};
use indexmap::IndexMap;
use std::collections::BTreeMap;

pub use prometheus_parser::*;

#[derive(Default)]
struct AggregatedHistogram {
    buckets: Vec<Bucket>,
    count: u32,
    sum: f64,
}

#[derive(Default)]
struct AggregatedSummary {
    quantiles: Vec<Quantile>,
    count: u32,
    sum: f64,
}

fn has_values_or_none(tags: BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

pub fn parse(packet: &str) -> Result<Vec<Metric>, ParserError> {
    let mut result = Vec::new();

    for group in prometheus_parser::group_metrics(packet)? {
        // just a header without measurements
        if group.metrics.is_empty() {
            continue;
        }

        match group.metrics {
            GroupKind::Counter(vec) => {
                for metric in vec {
                    let counter = Metric::new(
                        group.name.clone(),
                        None,
                        None,
                        has_values_or_none(metric.labels),
                        MetricKind::Absolute,
                        MetricValue::Counter {
                            value: metric.value,
                        },
                    );

                    result.push(counter);
                }
            }
            GroupKind::Gauge(vec) | GroupKind::Untyped(vec) => {
                for metric in vec {
                    let gauge = Metric::new(
                        group.name.clone(),
                        None,
                        None,
                        has_values_or_none(metric.labels),
                        MetricKind::Absolute,
                        MetricValue::Gauge {
                            value: metric.value,
                        },
                    );

                    result.push(gauge);
                }
            }
            GroupKind::Histogram(vec) => {
                let mut aggregates = IndexMap::<_, AggregatedHistogram>::new();

                for metric in vec {
                    let labels = metric.labels;
                    let aggregate = aggregates.entry(labels).or_default();
                    match metric.value {
                        HistogramMetricValue::Count { count } => {
                            aggregate.count = count;
                        }
                        HistogramMetricValue::Sum { sum } => {
                            aggregate.sum = sum;
                        }
                        HistogramMetricValue::Bucket { bucket, count } => {
                            // last bucket is implicit, because we store its value in 'count'
                            if bucket != f64::INFINITY {
                                let upper_limit = bucket;
                                aggregate.buckets.push(Bucket { upper_limit, count });
                            }
                        }
                    }
                }

                for (tags, mut aggregate) in aggregates {
                    for i in (1..aggregate.buckets.len()).rev() {
                        aggregate.buckets[i].count -= aggregate.buckets[i - 1].count;
                    }
                    let hist = Metric::new(
                        group.name.clone(),
                        None,
                        None,
                        has_values_or_none(tags),
                        MetricKind::Absolute,
                        MetricValue::AggregatedHistogram {
                            buckets: aggregate.buckets,
                            count: aggregate.count,
                            sum: aggregate.sum,
                        },
                    );

                    result.push(hist);
                }
            }
            GroupKind::Summary(vec) => {
                let mut aggregates = IndexMap::<_, AggregatedSummary>::new();

                for metric in vec {
                    let tags = metric.labels;
                    let aggregate = aggregates.entry(tags).or_default();

                    match metric.value {
                        SummaryMetricValue::Count { count } => {
                            aggregate.count = count;
                        }
                        SummaryMetricValue::Sum { sum } => {
                            aggregate.sum = sum;
                        }
                        SummaryMetricValue::Quantile { quantile, value } => {
                            let upper_limit = quantile;
                            aggregate.quantiles.push(Quantile { upper_limit, value });
                        }
                    }
                }

                for (tags, aggregate) in aggregates {
                    let summary = Metric::new(
                        group.name.clone(),
                        None,
                        None,
                        has_values_or_none(tags),
                        MetricKind::Absolute,
                        MetricValue::AggregatedSummary {
                            quantiles: aggregate.quantiles,
                            count: aggregate.count,
                            sum: aggregate.sum,
                        },
                    );

                    result.push(summary);
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod test {
    use super::parse;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use pretty_assertions::assert_eq;

    macro_rules! map {
        ($($key:expr => $value:expr),*) => {
            {
                let mut m = ::std::collections::BTreeMap::new();
                $(
                    m.insert($key.into(), $value.into());
                )*
                m
            }
        };
    }

    #[test]
    fn test_counter() {
        let exp = r##"
            # HELP uptime A counter
            # TYPE uptime counter
            uptime 123.0
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "uptime".into(),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Counter { value: 123.0 },
            )]),
        );
    }

    #[test]
    fn test_counter_empty() {
        let exp = r##"
            # HELP hidden A counter
            # TYPE hidden counter
            "##;

        assert_eq!(parse(exp), Ok(vec![]),);
    }

    #[test]
    fn test_counter_nan() {
        let exp = r##"
            # TYPE name counter
            name{labelname="val1",basename="basevalue"} NaN
            "##;

        match parse(exp).unwrap()[0].data.value {
            MetricValue::Counter { value } => {
                assert!(value.is_nan());
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_counter_weird() {
        let exp = r##"
            # A normal comment.
            #
            # TYPE name counter
            name {labelname="val2",basename="base\"v\\al\nue"} 0.23
            # HELP name two-line\n doc  str\\ing
            # HELP  name2  	doc str"ing 2
            #    TYPE    name2 counter
            name2{labelname="val2"	,basename   =   "basevalue2"		} +Inf
            name2{ labelname = "val1" , }-Inf
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "name".into(),
                    None,
                    None,
                    Some(
                        vec![
                            ("labelname".into(), "val2".into()),
                            ("basename".into(), "base\"v\\al\nue".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.23 },
                ),
                Metric::new(
                    "name2".into(),
                    None,
                    None,
                    Some(
                        vec![
                            ("labelname".into(), "val2".into()),
                            ("basename".into(), "basevalue2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: std::f64::INFINITY
                    },
                ),
                Metric::new(
                    "name2".into(),
                    None,
                    None,
                    Some(
                        vec![("labelname".into(), "val1".into()),]
                            .into_iter()
                            .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: std::f64::NEG_INFINITY
                    },
                ),
            ]),
        );
    }

    #[test]
    fn test_counter_tags_and_timestamp() {
        let exp = r##"
            # HELP http_requests_total The total number of HTTP requests.
            # TYPE http_requests_total counter
            http_requests_total{method="post",code="200"} 1027 1395066363000
            http_requests_total{method="post",code="400"}    3 1395066363000
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "http_requests_total".into(),
                    None,
                    None,
                    Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "200".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1027.0 },
                ),
                Metric::new(
                    "http_requests_total".into(),
                    None,
                    None,
                    Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "400".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 3.0 },
                )
            ]),
        );
    }

    #[test]
    fn test_gauge() {
        let exp = r##"
            # HELP latency A gauge
            # TYPE latency gauge
            latency 123.0
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "latency".into(),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Gauge { value: 123.0 },
            )]),
        );
    }

    #[test]
    fn test_gauge_minimalistic() {
        let exp = r##"
            metric_without_timestamp_and_labels 12.47
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "metric_without_timestamp_and_labels".into(),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Gauge { value: 12.47 },
            )]),
        );
    }

    #[test]
    fn test_gauge_empty_labels() {
        let exp = r##"
            no_labels{} 3
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "no_labels".into(),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::Gauge { value: 3.0 },
            )]),
        );
    }

    #[test]
    fn test_gauge_minimalistic_escaped() {
        let exp = r##"
            msdos_file_access_time_seconds{path="C:\\DIR\\FILE.TXT",error="Cannot find file:\n\"FILE.TXT\""} 1.458255915e9
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "msdos_file_access_time_seconds".into(),
                None,
                None,
                Some(
                    vec![
                        ("path".into(), "C:\\DIR\\FILE.TXT".into()),
                        ("error".into(), "Cannot find file:\n\"FILE.TXT\"".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: 1458255915.0
                },
            )]),
        );
    }

    #[test]
    fn test_tag_value_contain_bracket() {
        let exp = r##"
            # HELP name counter
            # TYPE name counter
            name{tag="}"} 0
            "##;
        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "name".into(),
                None,
                None,
                Some(map! {"tag" => "}"}),
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )]),
        );
    }

    #[test]
    fn test_parse_tag_value_contain_comma() {
        let exp = r##"
            # HELP name counter
            # TYPE name counter
            name{tag="a,b"} 0
            "##;
        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "name".into(),
                None,
                None,
                Some(map! {"tag" => "a,b"}),
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )]),
        );
    }

    #[test]
    fn test_parse_tag_escaping() {
        let exp = r##"
            # HELP name counter
            # TYPE name counter
            name{tag="\\n"} 0
            "##;
        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "name".into(),
                None,
                None,
                Some(map! {"tag" => "\\n"}),
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )]),
        );
    }

    #[test]
    fn test_parse_tag_dont_trim_value() {
        let exp = r##"
            # HELP name counter
            # TYPE name counter
            name{tag=" * "} 0
            "##;
        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "name".into(),
                None,
                None,
                Some(map! {"tag" => " * "}),
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )]),
        );
    }

    #[test]
    fn test_parse_tag_value_containing_equals() {
        let exp = r##"
            telemetry_scrape_size_bytes_count{registry="default",content_type="text/plain; version=0.0.4"} 1890
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "telemetry_scrape_size_bytes_count".into(),
                None,
                None,
                Some(
                    vec![
                        ("registry".into(), "default".into()),
                        ("content_type".into(), "text/plain; version=0.0.4".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge { value: 1890.0 },
            )]),
        );
    }

    #[test]
    fn test_parse_tag_error_no_value() {
        let exp = r##"
            telemetry_scrape_size_bytes_count{registry="default",content_type} 1890
            "##;

        assert!(parse(exp).is_err());
    }

    #[test]
    fn test_parse_tag_error_equals_empty_value() {
        let exp = r##"
            telemetry_scrape_size_bytes_count{registry="default",content_type=} 1890
            "##;

        assert!(parse(exp).is_err());
    }

    #[test]
    fn test_gauge_weird_timestamp() {
        let exp = r##"
            something_weird{problem="division by zero"} +Inf -3982045000
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "something_weird".into(),
                None,
                None,
                Some(
                    vec![("problem".into(), "division by zero".into())]
                        .into_iter()
                        .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: std::f64::INFINITY
                },
            )]),
        );
    }

    #[test]
    fn test_gauge_tabs() {
        let exp = r##"
            # TYPE	latency	gauge
            latency{env="production"}	1.0		1395066363000
            latency{env="testing"}		2.0		1395066363000
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "latency".into(),
                    None,
                    None,
                    Some(
                        vec![("env".into(), "production".into())]
                            .into_iter()
                            .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 1.0 },
                ),
                Metric::new(
                    "latency".into(),
                    None,
                    None,
                    Some(vec![("env".into(), "testing".into())].into_iter().collect()),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                )
            ]),
        );
    }

    #[test]
    fn test_mixed() {
        let exp = r##"
            # TYPE uptime counter
            uptime 123.0
            # TYPE temperature gauge
            temperature -1.5
            # TYPE launch_count counter
            launch_count 10.0
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "uptime".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 123.0 },
                ),
                Metric::new(
                    "temperature".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: -1.5 },
                ),
                Metric::new(
                    "launch_count".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 10.0 },
                )
            ]),
        );
    }

    #[test]
    fn test_no_value() {
        let exp = r##"
            # TYPE latency counter
            latency{env="production"}
            "##;

        assert!(parse(exp).is_err());
    }

    #[test]
    fn test_no_name() {
        let exp = r##"
            # TYPE uptime counter
            123.0
            "##;

        assert!(parse(exp).is_err());
    }

    #[test]
    fn test_mixed_and_loosely_typed() {
        let exp = r##"
            # TYPE uptime counter
            uptime 123.0
            last_downtime 4.0
            # TYPE temperature gauge
            temperature -1.5
            temperature_7_days_average 0.1
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "uptime".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 123.0 },
                ),
                Metric::new(
                    "last_downtime".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 4.0 },
                ),
                Metric::new(
                    "temperature".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: -1.5 },
                ),
                Metric::new(
                    "temperature_7_days_average".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 0.1 },
                )
            ]),
        );
    }

    #[test]
    fn test_histogram() {
        let exp = r##"
            # HELP http_request_duration_seconds A histogram of the request duration.
            # TYPE http_request_duration_seconds histogram
            http_request_duration_seconds_bucket{le="0.05"} 24054
            http_request_duration_seconds_bucket{le="0.1"} 33444
            http_request_duration_seconds_bucket{le="0.2"} 100392
            http_request_duration_seconds_bucket{le="0.5"} 129389
            http_request_duration_seconds_bucket{le="1"} 133988
            http_request_duration_seconds_bucket{le="+Inf"} 144320
            http_request_duration_seconds_sum 53423
            http_request_duration_seconds_count 144320
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric::new(
                "http_request_duration_seconds".into(),
                None,
                None,
                None,
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: crate::buckets![
                        0.05 => 24054, 0.1 => 9390, 0.2 => 66948, 0.5 => 28997, 1.0 => 4599
                    ],
                    count: 144320,
                    sum: 53423.0,
                },
            )]),
        );
    }

    #[test]
    fn test_histogram_with_labels() {
        let exp = r##"
            # HELP gitlab_runner_job_duration_seconds Histogram of job durations
            # TYPE gitlab_runner_job_duration_seconds histogram
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="30"} 327
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="60"} 474
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="300"} 535
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="600"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="1800"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="3600"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="7200"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="10800"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="18000"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="36000"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="z",le="+Inf"} 536
            gitlab_runner_job_duration_seconds_sum{runner="z"} 19690.129384881966
            gitlab_runner_job_duration_seconds_count{runner="z"} 536
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="30"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="60"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="300"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="600"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="1800"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="3600"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="7200"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="10800"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="18000"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="36000"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="x",le="+Inf"} 1
            gitlab_runner_job_duration_seconds_sum{runner="x"} 28.975436316
            gitlab_runner_job_duration_seconds_count{runner="x"} 1
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="30"} 285
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="60"} 1165
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="300"} 3071
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="600"} 3151
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="1800"} 3252
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="3600"} 3255
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="7200"} 3255
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="10800"} 3255
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="18000"} 3255
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="36000"} 3255
            gitlab_runner_job_duration_seconds_bucket{runner="y",le="+Inf"} 3255
            gitlab_runner_job_duration_seconds_sum{runner="y"} 381111.7498891335
            gitlab_runner_job_duration_seconds_count{runner="y"} 3255
        "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "gitlab_runner_job_duration_seconds".into(),
                    None,
                    None,
                    Some(vec![("runner".into(), "z".into())].into_iter().collect()),
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: crate::buckets![
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
                ),
                Metric::new(
                    "gitlab_runner_job_duration_seconds".into(),
                    None,
                    None,
                    Some(vec![("runner".into(), "x".into())].into_iter().collect()),
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: crate::buckets![
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
                ),
                Metric::new(
                    "gitlab_runner_job_duration_seconds".into(),
                    None,
                    None,
                    Some(vec![("runner".into(), "y".into())].into_iter().collect()),
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: crate::buckets![
                            30.0 => 285, 60.0 => 880, 300.0 => 1906, 600.0 => 80, 1800.0 => 101, 3600.0 => 3,
                            7200.0 => 0, 10800.0 => 0, 18000.0 => 0, 36000.0 => 0
                        ],
                        count: 3255,
                        sum: 381111.7498891335,
                    },
                )
            ]),
        );
    }

    #[test]
    fn test_summary() {
        let exp = r##"
            # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
            # TYPE rpc_duration_seconds summary
            rpc_duration_seconds{service="a",quantile="0.01"} 3102
            rpc_duration_seconds{service="a",quantile="0.05"} 3272
            rpc_duration_seconds{service="a",quantile="0.5"} 4773
            rpc_duration_seconds{service="a",quantile="0.9"} 9001
            rpc_duration_seconds{service="a",quantile="0.99"} 76656
            rpc_duration_seconds_sum{service="a"} 1.7560473e+07
            rpc_duration_seconds_count{service="a"} 2693
            # HELP go_gc_duration_seconds A summary of the GC invocation durations.
            # TYPE go_gc_duration_seconds summary
            go_gc_duration_seconds{quantile="0"} 0.009460965
            go_gc_duration_seconds{quantile="0.25"} 0.009793382
            go_gc_duration_seconds{quantile="0.5"} 0.009870205
            go_gc_duration_seconds{quantile="0.75"} 0.01001838
            go_gc_duration_seconds{quantile="1"} 0.018827136
            go_gc_duration_seconds_sum 4668.551713715
            go_gc_duration_seconds_count 602767
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "rpc_duration_seconds".into(),
                    None,
                    None,
                    Some(vec![("service".into(), "a".into())].into_iter().collect()),
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: crate::quantiles![
                            0.01 => 3102.0,
                            0.05 => 3272.0,
                            0.5 => 4773.0,
                            0.9 => 9001.0,
                            0.99 => 76656.0
                        ],
                        count: 2693,
                        sum: 1.7560473e+07,
                    },
                ),
                Metric::new(
                    "go_gc_duration_seconds".into(),
                    None,
                    None,
                    None,
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: crate::quantiles![
                            0.0 => 0.009460965,
                            0.25 => 0.009793382,
                            0.5 => 0.009870205,
                            0.75 => 0.01001838,
                            1.0 => 0.018827136
                        ],
                        count: 602767,
                        sum: 4668.551713715,
                    },
                ),
            ]),
        );
    }

    // https://github.com/timberio/vector/issues/3276
    #[test]
    fn test_nginx() {
        let exp = r##"
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
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "in", "host" => "*"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 263719.0 },
                ),
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "in", "host" => "_"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 255061.0 },
                ),
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "in", "host" => "nginx-vts-status"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 8658.0 },
                ),
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "out", "host" => "*"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 944199.0 },
                ),
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "out", "host" => "_"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 360775.0 },
                ),
                Metric::new(
                    "nginx_server_bytes".into(),
                    None,
                    None,
                    Some(map! {"direction" => "out", "host" => "nginx-vts-status"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 583424.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "bypass"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "expired"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "hit"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "miss"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "revalidated"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "nginx_server_cache".into(),
                    None,
                    None,
                    Some(map! {"host" => "*", "status" => "scarce"}),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                )
            ])
        );
    }
}

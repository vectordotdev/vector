use crate::event::metric::{Metric, MetricKind, MetricValue};
use indexmap::IndexMap;
use std::collections::BTreeMap;

pub use prometheus_parser::*;

struct ParserAggregate {
    name: String,
    bounds: Vec<f64>,
    values: Vec<f64>,
    count: u32,
    sum: f64,
    tags: BTreeMap<String, String>,
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
                    let tags = if !metric.labels.is_empty() {
                        Some(metric.labels)
                    } else {
                        None
                    };

                    let counter = Metric {
                        name: group.name.clone(),
                        timestamp: None,
                        tags,
                        kind: MetricKind::Absolute,
                        value: MetricValue::Counter {
                            value: metric.value,
                        },
                    };

                    result.push(counter);
                }
            }
            GroupKind::Gauge(vec) | GroupKind::Untyped(vec) => {
                for metric in vec {
                    let tags = if !metric.labels.is_empty() {
                        Some(metric.labels)
                    } else {
                        None
                    };

                    let gauge = Metric {
                        name: group.name.clone(),
                        timestamp: None,
                        tags,
                        kind: MetricKind::Absolute,
                        value: MetricValue::Gauge {
                            value: metric.value,
                        },
                    };

                    result.push(gauge);
                }
            }
            GroupKind::Histogram(vec) => {
                let mut aggregates = IndexMap::new();

                for metric in vec {
                    let tags = metric.get_labels().clone();
                    let aggregate = aggregates.entry(tags.clone()).or_insert(ParserAggregate {
                        name: group.name.clone(),
                        bounds: Vec::new(),
                        values: Vec::new(),
                        count: 0,
                        sum: 0.0,
                        tags,
                    });

                    match metric {
                        HistogramMetric::Count { value, .. } => {
                            aggregate.count = value as u32; // TODO: check
                        }
                        HistogramMetric::Sum { value, .. } => {
                            aggregate.sum = value;
                        }
                        HistogramMetric::Bucket { bucket, value, .. } => {
                            // last bucket is implicit, because we store its value in 'count'
                            if bucket != f64::INFINITY {
                                aggregate.bounds.push(bucket);
                                aggregate.values.push(value);
                            }
                        }
                    }
                }

                for (_, aggregate) in aggregates {
                    let tags = if !aggregate.tags.is_empty() {
                        Some(aggregate.tags)
                    } else {
                        None
                    };

                    let hist = Metric {
                        name: aggregate.name,
                        timestamp: None,
                        tags,
                        kind: MetricKind::Absolute,
                        value: MetricValue::AggregatedHistogram {
                            buckets: aggregate.bounds,
                            counts: aggregate.values.into_iter().map(|x| x as u32).collect(), // TODO: check
                            count: aggregate.count,
                            sum: aggregate.sum,
                        },
                    };

                    result.push(hist);
                }
            }
            GroupKind::Summary(vec) => {
                let mut aggregates = IndexMap::new();

                for metric in vec {
                    let tags = metric.get_labels().clone();
                    let aggregate = aggregates.entry(tags.clone()).or_insert(ParserAggregate {
                        name: group.name.clone(),
                        bounds: Vec::new(),
                        values: Vec::new(),
                        count: 0,
                        sum: 0.0,
                        tags,
                    });

                    match metric {
                        SummaryMetric::Count { value, .. } => {
                            aggregate.count = value as u32; // TODO: check
                        }
                        SummaryMetric::Sum { value, .. } => {
                            aggregate.sum = value;
                        }
                        SummaryMetric::Quantile {
                            quantile, value, ..
                        } => {
                            aggregate.bounds.push(quantile);
                            aggregate.values.push(value);
                        }
                    }
                }

                for (_, aggregate) in aggregates {
                    let tags = if !aggregate.tags.is_empty() {
                        Some(aggregate.tags)
                    } else {
                        None
                    };

                    let summary = Metric {
                        name: aggregate.name,
                        timestamp: None,
                        tags,
                        kind: MetricKind::Absolute,
                        value: MetricValue::AggregatedSummary {
                            quantiles: aggregate.bounds,
                            values: aggregate.values,
                            count: aggregate.count,
                            sum: aggregate.sum,
                        },
                    };

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
            Ok(vec![Metric {
                name: "uptime".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 123.0 },
            }]),
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

        match parse(exp).unwrap()[0].value {
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
                Metric {
                    name: "name".into(),
                    timestamp: None,
                    tags: Some(
                        vec![
                            ("labelname".into(), "val2".into()),
                            ("basename".into(), "base\"v\\al\nue".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.23 },
                },
                Metric {
                    name: "name2".into(),
                    timestamp: None,
                    tags: Some(
                        vec![
                            ("labelname".into(), "val2".into()),
                            ("basename".into(), "basevalue2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: std::f64::INFINITY
                    },
                },
                Metric {
                    name: "name2".into(),
                    timestamp: None,
                    tags: Some(
                        vec![("labelname".into(), "val1".into()),]
                            .into_iter()
                            .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: std::f64::NEG_INFINITY
                    },
                },
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
                Metric {
                    name: "http_requests_total".into(),
                    timestamp: None,
                    tags: Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "200".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 1027.0 },
                },
                Metric {
                    name: "http_requests_total".into(),
                    timestamp: None,
                    tags: Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "400".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 3.0 },
                }
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
            Ok(vec![Metric {
                name: "latency".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 123.0 },
            }]),
        );
    }

    #[test]
    fn test_gauge_minimalistic() {
        let exp = r##"
            metric_without_timestamp_and_labels 12.47
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric {
                name: "metric_without_timestamp_and_labels".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 12.47 },
            }]),
        );
    }

    #[test]
    fn test_gauge_empty_labels() {
        let exp = r##"
            no_labels{} 3
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric {
                name: "no_labels".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 3.0 },
            }]),
        );
    }

    #[test]
    fn test_gauge_minimalistic_escaped() {
        let exp = r##"
            msdos_file_access_time_seconds{path="C:\\DIR\\FILE.TXT",error="Cannot find file:\n\"FILE.TXT\""} 1.458255915e9
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric {
                name: "msdos_file_access_time_seconds".into(),
                timestamp: None,
                tags: Some(
                    vec![
                        ("path".into(), "C:\\DIR\\FILE.TXT".into()),
                        ("error".into(), "Cannot find file:\n\"FILE.TXT\"".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: 1458255915.0
                },
            }]),
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
            Ok(vec![Metric {
                name: "name".into(),
                timestamp: None,
                tags: Some(map! {"tag" => "}"}),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 0.0 },
            }]),
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
            Ok(vec![Metric {
                name: "name".into(),
                timestamp: None,
                tags: Some(map! {"tag" => "a,b"}),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 0.0 },
            }]),
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
            Ok(vec![Metric {
                name: "name".into(),
                timestamp: None,
                tags: Some(map! {"tag" => "\\n"}),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 0.0 },
            }]),
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
            Ok(vec![Metric {
                name: "name".into(),
                timestamp: None,
                tags: Some(map! {"tag" => " * "}),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 0.0 },
            }]),
        );
    }

    #[test]
    fn test_parse_tag_value_containing_equals() {
        let exp = r##"
            telemetry_scrape_size_bytes_count{registry="default",content_type="text/plain; version=0.0.4"} 1890
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![Metric {
                name: "telemetry_scrape_size_bytes_count".into(),
                timestamp: None,
                tags: Some(
                    vec![
                        ("registry".into(), "default".into()),
                        ("content_type".into(), "text/plain; version=0.0.4".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: 1890.0 },
            }]),
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
            Ok(vec![Metric {
                name: "something_weird".into(),
                timestamp: None,
                tags: Some(
                    vec![("problem".into(), "division by zero".into())]
                        .into_iter()
                        .collect()
                ),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: std::f64::INFINITY
                },
            }]),
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
                Metric {
                    name: "latency".into(),
                    timestamp: None,
                    tags: Some(
                        vec![("env".into(), "production".into())]
                            .into_iter()
                            .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 1.0 },
                },
                Metric {
                    name: "latency".into(),
                    timestamp: None,
                    tags: Some(vec![("env".into(), "testing".into())].into_iter().collect()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                }
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
                Metric {
                    name: "uptime".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 123.0 },
                },
                Metric {
                    name: "temperature".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: -1.5 },
                },
                Metric {
                    name: "launch_count".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 10.0 },
                }
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
                Metric {
                    name: "uptime".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 123.0 },
                },
                Metric {
                    name: "last_downtime".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 4.0 },
                },
                Metric {
                    name: "temperature".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: -1.5 },
                },
                Metric {
                    name: "temperature_7_days_average".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 0.1 },
                }
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
            Ok(vec![Metric {
                name: "http_request_duration_seconds".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::AggregatedHistogram {
                    buckets: vec![0.05, 0.1, 0.2, 0.5, 1.0],
                    counts: vec![24054, 33444, 100392, 129389, 133988],
                    count: 144320,
                    sum: 53423.0,
                },
            }]),
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
                Metric {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "z".into())].into_iter().collect()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![
                            30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0,
                            36000.0
                        ],
                        counts: vec![327, 474, 535, 536, 536, 536, 536, 536, 536, 536],
                        count: 536,
                        sum: 19690.129384881966,
                    },
                },
                Metric {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "x".into())].into_iter().collect()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![
                            30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0,
                            36000.0
                        ],
                        counts: vec![1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                        count: 1,
                        sum: 28.975436316,
                    },
                },
                Metric {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "y".into())].into_iter().collect()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![
                            30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0,
                            36000.0
                        ],
                        counts: vec![285, 1165, 3071, 3151, 3252, 3255, 3255, 3255, 3255, 3255],
                        count: 3255,
                        sum: 381111.7498891335,
                    },
                }
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
                Metric {
                    name: "rpc_duration_seconds".into(),
                    timestamp: None,
                    tags: Some(vec![("service".into(), "a".into())].into_iter().collect()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.01, 0.05, 0.5, 0.9, 0.99],
                        values: vec![3102.0, 3272.0, 4773.0, 9001.0, 76656.0],
                        count: 2693,
                        sum: 1.7560473e+07,
                    },
                },
                Metric {
                    name: "go_gc_duration_seconds".into(),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![0.0, 0.25, 0.5, 0.75, 1.0],
                        values: vec![
                            0.009460965,
                            0.009793382,
                            0.009870205,
                            0.01001838,
                            0.018827136
                        ],
                        count: 602767,
                        sum: 4668.551713715,
                    },
                },
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
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "in", "host" => "*"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 263719.0 }
                },
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "in", "host" => "_"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 255061.0 }
                },
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "in", "host" => "nginx-vts-status"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 8658.0 }
                },
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "out", "host" => "*"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 944199.0 }
                },
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "out", "host" => "_"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 360775.0 }
                },
                Metric {
                    name: "nginx_server_bytes".into(),
                    timestamp: None,
                    tags: Some(map! {"direction" => "out", "host" => "nginx-vts-status"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 583424.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "bypass"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "expired"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "hit"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "miss"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "revalidated"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                },
                Metric {
                    name: "nginx_server_cache".into(),
                    timestamp: None,
                    tags: Some(map! {"host" => "*", "status" => "scarce"}),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 }
                }
            ])
        );
    }
}

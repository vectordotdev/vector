use crate::event::metric::{Metric, MetricKind, MetricValue};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use snafu::{ResultExt, Snafu};
use std::collections::BTreeMap;
use std::num::ParseFloatError;

lazy_static! {
    static ref WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ParserType {
    Untyped,
    Counter,
    Gauge,
    Histogram,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParserHeader {
    name: String,
    kind: ParserType,
}

struct ParserMetric {
    name: String,
    value: f64,
    tags: BTreeMap<String, String>,
}

struct ParserAggregate {
    name: String,
    bounds: Vec<f64>,
    values: Vec<f64>,
    count: u32,
    sum: f64,
    tags: BTreeMap<String, String>,
}

fn is_header(input: &str) -> bool {
    input.starts_with("# TYPE")
}

fn is_comment(input: &str) -> bool {
    input.starts_with("#")
}

fn parse_header(input: &str) -> Result<ParserHeader, ParserError> {
    // example:
    // # TYPE uptime counter
    let tokens: Vec<_> = input.split_ascii_whitespace().collect();

    if tokens.len() != 4 {
        return Err(ParserError::Malformed {
            s: "expected 4 tokens in TYPE string",
        });
    }

    let name = tokens[2];
    let kind = match tokens[3] {
        "counter" => ParserType::Counter,
        "gauge" => ParserType::Gauge,
        "untyped" => ParserType::Untyped,
        "histogram" => ParserType::Histogram,
        "summary" => ParserType::Summary,
        other => {
            return Err(ParserError::UnknownMetricType {
                s: other.to_string(),
            });
        }
    };

    Ok(ParserHeader {
        name: name.to_owned(),
        kind,
    })
}

fn parse_value(input: &str) -> Result<f64, ParserError> {
    let input = input.trim();
    let value = match input {
        "Nan" => std::f64::NAN,
        "+Inf" => std::f64::INFINITY,
        "-Inf" => std::f64::NEG_INFINITY,
        s => s.parse().with_context(|| InvalidFloat { s: input })?,
    };

    Ok(value)
}

fn parse_tags(input: &str) -> Result<BTreeMap<String, String>, ParserError> {
    let input = input.trim();
    let mut result = BTreeMap::new();

    if input.is_empty() {
        return Ok(result);
    }

    let pairs = input.split(',').collect::<Vec<_>>();
    for pair in pairs {
        if pair.is_empty() {
            continue;
        }
        let pair = pair.trim();
        let parts = pair.split('=').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(ParserError::Malformed {
                s: "expected 2 values separated by '='",
            });
        }
        let key = parts[0].trim().to_string();
        let mut value = parts[1].trim();
        if value.starts_with('"') {
            value = &value[1..];
        };
        if value.ends_with('"') {
            value = &value[..value.len() - 1];
        };
        let value = value.trim();
        let value = value.replace(r#"\\"#, "\\");
        let value = value.replace(r#"\n"#, "\n");
        let value = value.replace(r#"\""#, "\"");
        result.insert(key, value);
    }

    Ok(result)
}

fn parse_metric(input: &str) -> Result<ParserMetric, ParserError> {
    // check if labels are present
    if let Some(pos) = input.find('}') {
        // example: http_requests_total{method="post",code="200"} 1027 1395066363000
        // first comes name and labels
        let (first, second) = input.split_at(pos);
        let parts = first.split('{').collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(ParserError::Malformed {
                s: "expected at least 2 tokens in data line",
            });
        };

        let name = parts[0].trim();
        let tags = parse_tags(parts[1])?;
        // second is value and optional timestamp, which is not meant to be used at client side
        let parts = &second[1..].trim().split(' ').collect::<Vec<_>>();
        let value = parse_value(parts[0])?;

        Ok(ParserMetric {
            name: name.to_string(),
            value,
            tags,
        })
    } else {
        // there are no labels
        // example: http_requests_total 1027 1395066363000
        let parts = input.split(' ').collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(ParserError::Malformed {
                s: "expected at least 2 tokens in data line",
            });
        };
        let name = parts[0];
        let value = parse_value(parts[1])?;

        Ok(ParserMetric {
            name: name.to_string(),
            value,
            tags: BTreeMap::new(),
        })
    }
}

fn group_metrics(packet: &str) -> Result<IndexMap<ParserHeader, Vec<String>>, ParserError> {
    // This will organise text into groups of lines, wrt to the format spec:
    // https://prometheus.io/docs/instrumenting/exposition_formats/#text-format-details
    //
    // All lines for a given metric must be provided as one single group,
    // with the optional HELP and TYPE lines first (in no particular order).
    // Beyond that, reproducible sorting in repeated expositions is preferred
    // but not required, i.e. do not sort if the computational cost is prohibitive.
    //
    // Each line must have a unique combination of a metric name and labels.
    // Otherwise, the ingestion behavior is undefined.
    let mut result = IndexMap::new();

    let mut current_header = ParserHeader {
        name: "".into(),
        kind: ParserType::Untyped,
    };

    for line in packet.lines() {
        let line = line.trim();
        let line: String = WHITESPACE.replace_all(&line, " ").to_string();

        if line.is_empty() {
            continue;
        }

        if is_header(&line) {
            // parse expected name and type from TYPE header
            let header = parse_header(&line)?;
            if !result.contains_key(&header) {
                result.insert(header.clone(), Vec::new());
            }
            // we will need it to analyse the consequent lines
            current_header = header;
        } else if is_comment(&line) {
            // skip comments and HELP strings
        } else {
            // parse the data line
            let metric = parse_metric(&line)?;

            current_header = match current_header.kind {
                ParserType::Histogram => {
                    // check if this is still a histogram
                    if metric.name == format!("{}_bucket", current_header.name)
                        || metric.name == format!("{}_count", current_header.name)
                        || metric.name == format!("{}_sum", current_header.name)
                    {
                        current_header
                    } else {
                        // nope it's a new unrelated metric
                        ParserHeader {
                            name: metric.name,
                            kind: ParserType::Untyped,
                        }
                    }
                }
                ParserType::Summary => {
                    // check if this is still a summary
                    if metric.name == current_header.name
                        || metric.name == format!("{}_count", current_header.name)
                        || metric.name == format!("{}_sum", current_header.name)
                    {
                        current_header
                    } else {
                        ParserHeader {
                            name: metric.name,
                            kind: ParserType::Untyped,
                        }
                    }
                }
                _ => {
                    if metric.name == current_header.name {
                        current_header
                    } else {
                        ParserHeader {
                            name: metric.name,
                            kind: ParserType::Untyped,
                        }
                    }
                }
            };

            if let Some(lines) = result.get_mut(&current_header) {
                lines.push(line);
            } else {
                result.insert(current_header.clone(), vec![line]);
            }
        }
    }

    Ok(result)
}

pub fn parse(packet: &str) -> Result<Vec<Metric>, ParserError> {
    let mut result = Vec::new();

    for (header, group) in group_metrics(packet)? {
        // just a header without measurements
        if group.is_empty() {
            continue;
        }

        match header.kind {
            ParserType::Counter => {
                for line in group {
                    let metric = parse_metric(&line)?;
                    let tags = if !metric.tags.is_empty() {
                        Some(metric.tags)
                    } else {
                        None
                    };

                    let counter = Metric {
                        name: metric.name,
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
            ParserType::Gauge | ParserType::Untyped => {
                for line in group {
                    let metric = parse_metric(&line)?;
                    let tags = if !metric.tags.is_empty() {
                        Some(metric.tags)
                    } else {
                        None
                    };

                    let gauge = Metric {
                        name: metric.name,
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
            ParserType::Histogram => {
                let mut aggregates = IndexMap::new();

                for line in group {
                    let metric = parse_metric(&line)?;
                    let mut tags = metric.tags;
                    let bucket = tags.remove("le");

                    let v: Vec<_> = metric.name.rsplitn(2, '_').collect();
                    if v.len() < 2 {
                        return Err(ParserError::Malformed {
                            s: "expected histogram name suffix",
                        });
                    }
                    let (name, suffix) = (v[1], v[0]);

                    let mut id: Vec<_> = tags.iter().collect();
                    id.sort();
                    let group_key = format!("{:?}", id);

                    let aggregate = aggregates.entry(group_key).or_insert(ParserAggregate {
                        name: name.to_owned(),
                        bounds: Vec::new(),
                        values: Vec::new(),
                        count: 0,
                        sum: 0.0,
                        tags,
                    });

                    match suffix {
                        "bucket" => {
                            if let Some(b) = bucket {
                                // last bucket is implicit, because we store its value in 'count'
                                if b != "+Inf" {
                                    aggregate.bounds.push(parse_value(&b)?);
                                    aggregate.values.push(metric.value);
                                }
                            } else {
                                return Err(ParserError::Malformed {
                                    s: "expected \"le\" tag in histogram bucket",
                                });
                            }
                        }
                        "sum" => {
                            aggregate.sum = metric.value;
                        }
                        "count" => {
                            aggregate.count = metric.value as u32;
                        }
                        _ => {
                            return Err(ParserError::Malformed {
                                s: "unknown histogram name suffix",
                            });
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
                            counts: aggregate.values.into_iter().map(|x| x as u32).collect(),
                            count: aggregate.count,
                            sum: aggregate.sum,
                        },
                    };

                    result.push(hist);
                }
            }
            ParserType::Summary => {
                let mut aggregates = IndexMap::new();

                for line in group {
                    let metric = parse_metric(&line)?;
                    let mut tags = metric.tags;
                    let bucket = tags.remove("quantile");

                    let (name, suffix) =
                        if metric.name.ends_with("_sum") || metric.name.ends_with("_count") {
                            let v: Vec<_> = metric.name.rsplitn(2, '_').collect();
                            (v[1], v[0])
                        } else {
                            (&metric.name[..], "")
                        };

                    let mut id: Vec<_> = tags.iter().collect();
                    id.sort();
                    let group_key = format!("{:?}", id);

                    let aggregate = aggregates.entry(group_key).or_insert(ParserAggregate {
                        name: name.to_owned(),
                        bounds: Vec::new(),
                        values: Vec::new(),
                        count: 0,
                        sum: 0.0,
                        tags,
                    });

                    match suffix {
                        "" => {
                            if let Some(b) = bucket {
                                aggregate.bounds.push(parse_value(&b)?);
                                aggregate.values.push(metric.value);
                            } else {
                                return Err(ParserError::Malformed {
                                    s: "expected \"quantile\" tag in summary bucket",
                                });
                            }
                        }
                        "sum" => {
                            aggregate.sum = metric.value;
                        }
                        "count" => {
                            aggregate.count = metric.value as u32;
                        }
                        _ => {
                            return Err(ParserError::Malformed {
                                s: "unknown summary name suffix",
                            });
                        }
                    }
                }

                for (_id, aggregate) in aggregates {
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

#[derive(Debug, PartialEq, Snafu)]
pub enum ParserError {
    Malformed {
        s: &'static str,
    },
    UnknownMetricType {
        s: String,
    },
    #[snafu(display("Invalid floating point number {:?}: {}", s, source))]
    InvalidFloat {
        s: String,
        source: ParseFloatError,
    },
}

#[cfg(test)]
mod test {
    use super::parse;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use pretty_assertions::assert_eq;

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
}

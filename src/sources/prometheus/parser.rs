use crate::event::Metric;
use chrono::{offset::TimeZone, DateTime, Utc};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::{
    error, fmt,
    num::{ParseFloatError, ParseIntError},
};

lazy_static! {
    static ref WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ParserType {
    Untyped,
    Counter,
    Gauge,
    Histogram,
    // Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ParserHeader {
    name: String,
    kind: ParserType,
}

struct ParserMetric {
    id: String,
    name: String,
    value: f64,
    tags: Option<HashMap<String, String>>,
    timestamp: Option<DateTime<Utc>>,
}

struct ParserAggregate {
    // id: String,
    name: String,
    buckets: Vec<f64>,
    counts: Vec<u32>,
    count: u32,
    sum: f64,
    tags: Option<HashMap<String, String>>,
    // timestamp: Option<DateTime<Utc>>,
}

fn is_header(input: &str) -> bool {
    input.starts_with("# TYPE")
}

fn is_comment(input: &str) -> bool {
    input.starts_with("#")
}

fn parse_header(input: &str) -> Result<ParserHeader, ParserError> {
    // # TYPE uptime counter
    // 0 1    2      3
    let tokens: Vec<_> = input.split_ascii_whitespace().collect();

    if tokens.len() != 4 {
        return Err(ParserError::Malformed("expected 4 tokens in TYPE string"));
    }

    let name = tokens[2];
    let kind = match tokens[3] {
        "counter" => ParserType::Counter,
        "gauge" => ParserType::Gauge,
        "untyped" => ParserType::Untyped,
        "histogram" => ParserType::Histogram,
        "summary" => ParserType::Gauge,
        other => {
            return Err(ParserError::UnknownMetricType(other.to_string()));
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
        s => s.parse()?,
    };

    Ok(value)
}

fn parse_timestamp(input: &str) -> Result<DateTime<Utc>, ParserError> {
    let input = input.trim();
    let millis: i64 = input.parse()?;
    Ok(Utc.timestamp(millis / 1000, 0))
}

fn parse_tags(input: &str) -> Result<HashMap<String, String>, ParserError> {
    let input = input.trim();
    let mut result = HashMap::new();

    let pairs = input.split(',').collect::<Vec<_>>();
    for pair in pairs {
        let pair = pair.trim();
        let parts = pair.split('=').collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(ParserError::Malformed("expected 2 values separated by '='"));
        }
        let key = parts[0].to_string();
        let mut value = parts[1];
        if value.starts_with('"') {
            value = &value[1..];
        };
        if value.ends_with('"') {
            value = &value[..value.len() - 1];
        };
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
        let name = parts[0];
        let tags = parse_tags(parts[1])?;
        let tags = if !tags.is_empty() { Some(tags) } else { None };
        // second is value and optional timestamp
        let parts = &second[1..].trim().split(' ').collect::<Vec<_>>();
        let value = parse_value(parts[0])?;
        let timestamp = if let Some(ts) = parts.get(1) {
            Some(parse_timestamp(ts)?)
        } else {
            None
        };

        Ok(ParserMetric {
            id: first.to_string(),
            name: name.to_string(),
            value,
            tags,
            timestamp,
        })
    } else {
        // example: http_requests_total 1027 1395066363000
        let parts = input.split(' ').collect::<Vec<_>>();
        let name = parts[0];
        let value = parse_value(parts[1])?;
        let timestamp = if let Some(ts) = parts.get(2) {
            Some(parse_timestamp(ts)?)
        } else {
            None
        };

        Ok(ParserMetric {
            id: name.to_string(),
            name: name.to_string(),
            value,
            tags: None,
            timestamp,
        })
    }
}

fn group_metrics(packet: &str) -> Result<IndexMap<ParserHeader, Vec<String>>, ParserError> {
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
                    if metric.name.starts_with(&current_header.name)
                        && (metric.name.ends_with("_bucket")
                            || metric.name.ends_with("_count")
                            || metric.name.ends_with("_sum"))
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
    // https://prometheus.io/docs/instrumenting/exposition_formats/#text-format-details
    let mut result = Vec::new();
    // this will be used for deduplication
    let mut processed_metrics = HashSet::new();

    for (header, group) in group_metrics(packet)? {
        // just a header without measurements
        if group.is_empty() {
            continue;
        }

        match header.kind {
            ParserType::Counter => {
                for line in group {
                    let metric = parse_metric(&line)?;
                    if !processed_metrics.contains(&metric.id) {
                        let counter = Metric::Counter {
                            name: metric.name,
                            val: metric.value,
                            timestamp: metric.timestamp,
                            tags: metric.tags,
                        };
                        result.push(counter);
                        processed_metrics.insert(metric.id);
                    }
                }
            }
            ParserType::Gauge | ParserType::Untyped => {
                for line in group {
                    let metric = parse_metric(&line)?;
                    if !processed_metrics.contains(&metric.id) {
                        let gauge = Metric::Gauge {
                            name: metric.name,
                            val: metric.value,
                            direction: None,
                            timestamp: metric.timestamp,
                            tags: metric.tags,
                        };
                        result.push(gauge);
                        processed_metrics.insert(metric.id);
                    }
                }
            }
            ParserType::Histogram => {
                let mut aggregates = IndexMap::new();

                for line in group {
                    let metric = parse_metric(&line)?;
                    let mut tags = if let Some(tags) = metric.tags {
                        tags
                    } else {
                        HashMap::new()
                    };

                    let bucket = tags.remove("le");

                    let v: Vec<_> = metric.name.rsplitn(2, '_').collect();
                    if v.len() < 2 {
                        return Err(ParserError::Malformed("expected histogram name suffix"));
                    }

                    let mut id: Vec<_> = tags.iter().collect();
                    id.sort();
                    let id = format!("{:?}{:?}", v[1], id);

                    let tags = if !tags.is_empty() { Some(tags) } else { None };

                    let aggregate = aggregates.entry(id.clone()).or_insert(ParserAggregate {
                        name: v[1].to_owned(),
                        buckets: Vec::new(),
                        counts: Vec::new(),
                        count: 0,
                        sum: 0.0,
                        tags,
                    });

                    match v[0] {
                        "bucket" => {
                            if let Some(b) = bucket {
                                // last bucket is implicit, because we store its value in 'count'
                                if b != "+Inf" {
                                    aggregate.buckets.push(parse_value(&b)?);
                                    aggregate.counts.push(metric.value as u32);
                                }
                            } else {
                                return Err(ParserError::Malformed(
                                    "expected \"le\" tag in histogram bucket",
                                ));
                            }
                        }
                        "sum" => {
                            aggregate.sum = metric.value;
                        }
                        "count" => {
                            aggregate.count = metric.value as u32;
                        }
                        _ => {
                            return Err(ParserError::Malformed("unknown histogram name prefix"));
                        }
                    }
                }

                for (id, aggregate) in aggregates {
                    if !processed_metrics.contains(&id) {
                        let hist = Metric::AggregatedHistogram {
                            name: aggregate.name,
                            buckets: aggregate.buckets,
                            counts: (0..).zip(aggregate.counts.into_iter()).collect(),
                            count: aggregate.count,
                            sum: aggregate.sum,
                            stats: None,
                            timestamp: None,
                            tags: aggregate.tags,
                        };
                        result.push(hist);
                        processed_metrics.insert(id);
                    }
                }
            }
        }
    }

    Ok(result)
}

#[derive(Debug, PartialEq)]
pub enum ParserError {
    Malformed(&'static str),
    UnknownMetricType(String),
    InvalidInteger(ParseIntError),
    InvalidFloat(ParseFloatError),
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            anything => write!(f, "Prometheus parse error: {:?}", anything),
        }
    }
}

impl error::Error for ParserError {}

impl From<ParseIntError> for ParserError {
    fn from(e: ParseIntError) -> ParserError {
        ParserError::InvalidInteger(e)
    }
}

impl From<ParseFloatError> for ParserError {
    fn from(e: ParseFloatError) -> ParserError {
        ParserError::InvalidFloat(e)
    }
}

#[cfg(test)]
mod test {
    use super::parse;
    use crate::event::Metric;
    use chrono::{offset::TimeZone, Utc};
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
            Ok(vec![Metric::Counter {
                name: "uptime".into(),
                val: 123.0,
                timestamp: None,
                tags: None,
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
                Metric::Counter {
                    name: "http_requests_total".into(),
                    val: 1027.0,
                    timestamp: Some(Utc.ymd(2014, 3, 17).and_hms_nano(14, 26, 3, 0)),
                    tags: Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "200".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                },
                Metric::Counter {
                    name: "http_requests_total".into(),
                    val: 3.0,
                    timestamp: Some(Utc.ymd(2014, 3, 17).and_hms_nano(14, 26, 3, 0)),
                    tags: Some(
                        vec![
                            ("method".into(), "post".into()),
                            ("code".into(), "400".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
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
            Ok(vec![Metric::Gauge {
                name: "latency".into(),
                val: 123.0,
                direction: None,
                timestamp: None,
                tags: None,
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
            Ok(vec![Metric::Gauge {
                name: "metric_without_timestamp_and_labels".into(),
                val: 12.47,
                direction: None,
                timestamp: None,
                tags: None,
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
            Ok(vec![Metric::Gauge {
                name: "msdos_file_access_time_seconds".into(),
                val: 1458255915.0,
                direction: None,
                timestamp: None,
                tags: Some(
                    vec![
                        ("path".into(), "C:\\DIR\\FILE.TXT".into()),
                        ("error".into(), "Cannot find file:\n\"FILE.TXT\"".into())
                    ]
                    .into_iter()
                    .collect()
                ),
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
            Ok(vec![Metric::Gauge {
                name: "something_weird".into(),
                val: std::f64::INFINITY,
                direction: None,
                timestamp: Some(Utc.ymd(1969, 11, 15).and_hms_nano(21, 52, 35, 0)),
                tags: Some(
                    vec![("problem".into(), "division by zero".into())]
                        .into_iter()
                        .collect()
                ),
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
                Metric::Gauge {
                    name: "latency".into(),
                    val: 1.0,
                    direction: None,
                    timestamp: Some(Utc.ymd(2014, 3, 17).and_hms_nano(14, 26, 3, 0)),
                    tags: Some(
                        vec![("env".into(), "production".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "latency".into(),
                    val: 2.0,
                    direction: None,
                    timestamp: Some(Utc.ymd(2014, 3, 17).and_hms_nano(14, 26, 3, 0)),
                    tags: Some(vec![("env".into(), "testing".into())].into_iter().collect()),
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
                Metric::Counter {
                    name: "uptime".into(),
                    val: 123.0,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "temperature".into(),
                    val: -1.5,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
                Metric::Counter {
                    name: "launch_count".into(),
                    val: 10.0,
                    timestamp: None,
                    tags: None,
                }
            ]),
        );
    }

    #[test]
    fn test_mixed_and_duplicated() {
        let exp = r##"
            # TYPE uptime counter
            uptime 123.0
            uptime 234.0
            # TYPE temperature gauge
            temperature -1.5
            # TYPE uptime counter
            uptime 345.0
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::Counter {
                    name: "uptime".into(),
                    val: 123.0,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "temperature".into(),
                    val: -1.5,
                    direction: None,
                    timestamp: None,
                    tags: None,
                }
            ]),
        );
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
                Metric::Counter {
                    name: "uptime".into(),
                    val: 123.0,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "last_downtime".into(),
                    val: 4.0,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "temperature".into(),
                    val: -1.5,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "temperature_7_days_average".into(),
                    val: 0.1,
                    direction: None,
                    timestamp: None,
                    tags: None,
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
            Ok(vec![Metric::AggregatedHistogram {
                name: "http_request_duration_seconds".into(),
                buckets: vec![0.05, 0.1, 0.2, 0.5, 1.0],
                counts: vec![
                    (0, 24054),
                    (1, 33444),
                    (2, 100392),
                    (3, 129389),
                    (4, 133988)
                ]
                .into_iter()
                .collect(),
                count: 144320,
                sum: 53423.0,
                stats: None,
                timestamp: None,
                tags: None,
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
                Metric::AggregatedHistogram {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    buckets: vec![
                        30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0, 36000.0
                    ],
                    counts: vec![
                        (0, 327),
                        (1, 474),
                        (2, 535),
                        (3, 536),
                        (4, 536),
                        (5, 536),
                        (6, 536),
                        (7, 536),
                        (8, 536),
                        (9, 536),
                    ]
                    .into_iter()
                    .collect(),
                    count: 536,
                    sum: 19690.129384881966,
                    stats: None,
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "z".into())].into_iter().collect()),
                },
                Metric::AggregatedHistogram {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    buckets: vec![
                        30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0, 36000.0
                    ],
                    counts: vec![
                        (0, 1),
                        (1, 1),
                        (2, 1),
                        (3, 1),
                        (4, 1),
                        (5, 1),
                        (6, 1),
                        (7, 1),
                        (8, 1),
                        (9, 1),
                    ]
                    .into_iter()
                    .collect(),
                    count: 1,
                    sum: 28.975436316,
                    stats: None,
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "x".into())].into_iter().collect()),
                },
                Metric::AggregatedHistogram {
                    name: "gitlab_runner_job_duration_seconds".into(),
                    buckets: vec![
                        30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 18000.0, 36000.0
                    ],
                    counts: vec![
                        (0, 285),
                        (1, 1165),
                        (2, 3071),
                        (3, 3151),
                        (4, 3252),
                        (5, 3255),
                        (6, 3255),
                        (7, 3255),
                        (8, 3255),
                        (9, 3255),
                    ]
                    .into_iter()
                    .collect(),
                    count: 3255,
                    sum: 381111.7498891335,
                    stats: None,
                    timestamp: None,
                    tags: Some(vec![("runner".into(), "y".into())].into_iter().collect()),
                }
            ]),
        );
    }

    #[test]
    fn test_summary() {
        let exp = r##"
            # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
            # TYPE rpc_duration_seconds summary
            rpc_duration_seconds{quantile="0.01"} 3102
            rpc_duration_seconds{quantile="0.05"} 3272
            rpc_duration_seconds{quantile="0.5"} 4773
            rpc_duration_seconds{quantile="0.9"} 9001
            rpc_duration_seconds{quantile="0.99"} 76656
            rpc_duration_seconds_sum 1.7560473e+07
            rpc_duration_seconds_count 2693
            "##;

        assert_eq!(
            parse(exp),
            Ok(vec![
                Metric::Gauge {
                    name: "rpc_duration_seconds".into(),
                    val: 3102.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(
                        vec![("quantile".into(), "0.01".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds".into(),
                    val: 3272.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(
                        vec![("quantile".into(), "0.05".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds".into(),
                    val: 4773.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(
                        vec![("quantile".into(), "0.5".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds".into(),
                    val: 9001.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(
                        vec![("quantile".into(), "0.9".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds".into(),
                    val: 76656.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(
                        vec![("quantile".into(), "0.99".into())]
                            .into_iter()
                            .collect()
                    ),
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds_sum".into(),
                    val: 17560473.0,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "rpc_duration_seconds_count".into(),
                    val: 2693.0,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
            ]),
        );
    }
}

use crate::event::Metric;
use chrono::{offset::TimeZone, DateTime, Utc};
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

enum ParserType {
    Counter,
    Gauge,
    // Histogram,
    // Summary,
}

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
        "gauge" | "untyped" => ParserType::Gauge,
        "histogram" => ParserType::Gauge,
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
    // check is labels are present
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

pub fn parse(packet: &str) -> Result<Vec<Metric>, ParserError> {
    // https://prometheus.io/docs/instrumenting/exposition_formats/#text-format-details

    let mut result = Vec::new();
    // this will store information parsed from headers
    let mut known_types = HashMap::new();
    // this will be used for deduplication
    let mut processed_metrics = HashSet::new();

    for line in packet.lines() {
        let line = line.trim();
        let line = WHITESPACE.replace_all(&line, " ");

        if line.is_empty() {
            continue;
        }

        if is_header(&line) {
            // parse expected name and type from TYPE header
            let header = parse_header(&line)?;
            known_types.insert(header.name, header.kind);
            continue;
        } else if is_comment(&line) {
            // skip comments and HELP strings
            continue;
        } else {
            let metric = parse_metric(&line)?;
            // skip duplicates
            if !processed_metrics.contains(&metric.id) {
                let kind = known_types.get(&metric.name).unwrap_or(&ParserType::Gauge);
                let m = match kind {
                    ParserType::Counter => Metric::Counter {
                        name: metric.name,
                        val: metric.value,
                        timestamp: metric.timestamp,
                        tags: metric.tags,
                    },
                    ParserType::Gauge => Metric::Gauge {
                        name: metric.name,
                        val: metric.value,
                        direction: None,
                        timestamp: metric.timestamp,
                        tags: metric.tags,
                    },
                };
                result.push(m);
                processed_metrics.insert(metric.id);
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
            Ok(vec![
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 24054.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "0.05".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 33444.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "0.1".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 100392.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "0.2".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 129389.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "0.5".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 133988.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "1".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_bucket".into(),
                    val: 144320.0,
                    direction: None,
                    timestamp: None,
                    tags: Some(vec![("le".into(), "+Inf".into())].into_iter().collect()),
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_sum".into(),
                    val: 53423.0,
                    direction: None,
                    timestamp: None,
                    tags: None,
                },
                Metric::Gauge {
                    name: "http_request_duration_seconds_count".into(),
                    val: 144320.0,
                    direction: None,
                    timestamp: None,
                    tags: None,
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

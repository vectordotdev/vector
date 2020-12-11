use snafu::ResultExt;
use std::collections::BTreeMap;

mod line;

pub use line::ErrorKind;
use line::Line;
use line::Metric;
use line::MetricKind;

#[derive(Debug, snafu::Snafu, PartialEq)]
pub enum ParserError {
    #[snafu(display("{}, line: `{}`", kind, line))]
    WithLine {
        line: String,
        #[snafu(source)]
        kind: ErrorKind,
    },
    #[snafu(display("expected \"le\" tag for histogram metric"))]
    ExpectedLeTag,
    #[snafu(display("expected \"quantile\" tag for summary metric"))]
    ExpectedQuantileTag,

    #[snafu(display("error parsing label value: {}", error))]
    ParseLabelValue {
        #[snafu(source)]
        error: ErrorKind,
    },

    #[snafu(display("expected value in range [0, {}], found: {}", u32::MAX, value))]
    ValueOutOfRange { value: f64 },
}

#[derive(Debug, PartialEq)]
pub struct SummaryMetric {
    pub labels: BTreeMap<String, String>,
    pub value: SummaryMetricValue,
    pub timestamp: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum SummaryMetricValue {
    Quantile { quantile: f64, value: f64 },
    Sum { sum: f64 },
    Count { count: u32 },
}

#[derive(Debug, PartialEq)]
pub struct HistogramMetric {
    pub labels: BTreeMap<String, String>,
    pub value: HistogramMetricValue,
    pub timestamp: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum HistogramMetricValue {
    Bucket { bucket: f64, count: u32 },
    Sum { sum: f64 },
    Count { count: u32 },
}

#[derive(Debug, PartialEq)]
pub struct OtherMetric {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    pub timestamp: Option<i64>,
}

#[derive(Debug, PartialEq)]
pub enum GroupKind {
    Summary(Vec<SummaryMetric>),
    Histogram(Vec<HistogramMetric>),
    Gauge(Vec<OtherMetric>),
    Counter(Vec<OtherMetric>),
    Untyped(Vec<OtherMetric>),
}

#[derive(Debug, PartialEq)]
pub struct MetricGroup {
    pub name: String,
    pub metrics: GroupKind,
}

impl GroupKind {
    pub fn is_empty(&self) -> bool {
        match self {
            GroupKind::Counter(vec) | GroupKind::Gauge(vec) | GroupKind::Untyped(vec) => {
                vec.is_empty()
            }
            GroupKind::Histogram(vec) => vec.is_empty(),
            GroupKind::Summary(vec) => vec.is_empty(),
        }
    }
}

fn try_f64_to_u32(f: f64) -> Result<u32, ParserError> {
    if 0.0 <= f && f <= u32::MAX as f64 {
        Ok(f as u32)
    } else {
        Err(ParserError::ValueOutOfRange { value: f })
    }
}

impl MetricGroup {
    fn new(name: String, kind: MetricKind) -> Self {
        let metrics = match kind {
            MetricKind::Histogram => GroupKind::Histogram(Vec::new()),
            MetricKind::Summary => GroupKind::Summary(Vec::new()),
            MetricKind::Counter => GroupKind::Counter(Vec::new()),
            MetricKind::Gauge => GroupKind::Gauge(Vec::new()),
            MetricKind::Untyped => GroupKind::Untyped(Vec::new()),
        };
        MetricGroup { name, metrics }
    }

    // For cases where a metric group was not defined with `# TYPE ...`.
    fn new_untyped(metric: Metric) -> Self {
        let Metric {
            name,
            labels,
            value,
            timestamp,
        } = metric;
        MetricGroup {
            name,
            metrics: GroupKind::Untyped(vec![OtherMetric {
                labels,
                value,
                timestamp,
            }]),
        }
    }

    /// Err(_) if there are irrecoverable error.
    /// Ok(Some(metric)) if this metric belongs to another group.
    /// Ok(None) pushed successfully.
    fn try_push(&mut self, metric: Metric) -> Result<Option<Metric>, ParserError> {
        if !metric.name.starts_with(&self.name) {
            return Ok(Some(metric));
        }
        let suffix = &metric.name[self.name.len()..];

        match self.metrics {
            GroupKind::Counter(ref mut vec)
            | GroupKind::Gauge(ref mut vec)
            | GroupKind::Untyped(ref mut vec) => {
                if !suffix.is_empty() {
                    return Ok(Some(metric));
                }
                vec.push(OtherMetric {
                    labels: metric.labels,
                    value: metric.value,
                    timestamp: metric.timestamp,
                });
            }
            GroupKind::Histogram(ref mut vec) => match suffix {
                "_bucket" => {
                    let mut labels = metric.labels;
                    let bucket = labels.remove("le").ok_or(ParserError::ExpectedLeTag)?;
                    let (_, bucket) = line::Metric::parse_value(&bucket)
                        .map_err(Into::into)
                        .context(ParseLabelValue)?;
                    vec.push(HistogramMetric {
                        labels,
                        value: HistogramMetricValue::Bucket {
                            bucket,
                            count: try_f64_to_u32(metric.value)?,
                        },
                        timestamp: metric.timestamp,
                    });
                }
                "_sum" => vec.push(HistogramMetric {
                    value: HistogramMetricValue::Sum { sum: metric.value },
                    labels: metric.labels,
                    timestamp: metric.timestamp,
                }),
                "_count" => vec.push(HistogramMetric {
                    value: HistogramMetricValue::Count {
                        count: try_f64_to_u32(metric.value)?,
                    },
                    labels: metric.labels,
                    timestamp: metric.timestamp,
                }),
                _ => return Ok(Some(metric)),
            },
            GroupKind::Summary(ref mut vec) => match suffix {
                "" => {
                    let mut labels = metric.labels;
                    let quantile = labels
                        .remove("quantile")
                        .ok_or(ParserError::ExpectedQuantileTag)?;
                    let (_, quantile) = line::Metric::parse_value(&quantile)
                        .map_err(Into::into)
                        .context(ParseLabelValue)?;
                    vec.push(SummaryMetric {
                        labels,
                        value: SummaryMetricValue::Quantile {
                            quantile,
                            value: metric.value,
                        },
                        timestamp: metric.timestamp,
                    });
                }
                "_sum" => vec.push(SummaryMetric {
                    value: SummaryMetricValue::Sum { sum: metric.value },
                    labels: metric.labels,
                    timestamp: metric.timestamp,
                }),
                "_count" => vec.push(SummaryMetric {
                    value: SummaryMetricValue::Count {
                        count: try_f64_to_u32(metric.value)?,
                    },
                    labels: metric.labels,
                    timestamp: metric.timestamp,
                }),
                _ => return Ok(Some(metric)),
            },
        }
        Ok(None)
    }
}

pub fn group_text_metrics(input: &str) -> Result<Vec<MetricGroup>, ParserError> {
    let mut groups = Vec::new();

    for line in input.lines() {
        let line = Line::parse(line).with_context(|| WithLine {
            line: line.to_owned(),
        })?;
        if let Some(line) = line {
            match line {
                Line::Header(header) => {
                    groups.push(MetricGroup::new(header.metric_name, header.kind));
                }
                Line::Metric(metric) => {
                    let metric = match groups.last_mut() {
                        Some(group) => group.try_push(metric)?,
                        None => Some(metric),
                    };
                    if let Some(metric) = metric {
                        groups.push(MetricGroup::new_untyped(metric));
                    }
                }
            }
        }
    }

    Ok(groups)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_group_text_metrics() {
        let input = r##"
            # HELP http_requests_total The total number of HTTP requests.
            # TYPE http_requests_total counter
            http_requests_total{method="post",code="200"} 1027 1395066363000
            http_requests_total{method="post",code="400"}    3 1395066363000

            # Escaping in label values:
            msdos_file_access_time_seconds{path="C:\\DIR\\FILE.TXT",error="Cannot find file:\n\"FILE.TXT\""} 1.458255915e9

            # Minimalistic line:
            metric_without_timestamp_and_labels 12.47

            # A weird metric from before the epoch:
            something_weird{problem="division by zero"} +Inf -3982045

            # A histogram, which has a pretty complex representation in the text format:
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

            # Finally a summary, which has a complex representation, too:
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
        group_text_metrics(input).unwrap();
    }

    #[test]
    fn test_f64_to_u32() {
        let value = -1.0;
        let error = try_f64_to_u32(value).unwrap_err();
        assert_eq!(error, ParserError::ValueOutOfRange { value });

        let value = u32::MAX as f64 + 1.0;
        let error = try_f64_to_u32(value).unwrap_err();
        assert_eq!(error, ParserError::ValueOutOfRange { value });

        let value = f64::NAN;
        let error = try_f64_to_u32(value).unwrap_err();
        assert!(matches!(error, ParserError::ValueOutOfRange { value } if value.is_nan()));

        let value = f64::INFINITY;
        let error = try_f64_to_u32(value).unwrap_err();
        assert_eq!(error, ParserError::ValueOutOfRange { value });

        let value = f64::NEG_INFINITY;
        let error = try_f64_to_u32(value).unwrap_err();
        assert_eq!(error, ParserError::ValueOutOfRange { value });

        assert_eq!(try_f64_to_u32(0.0).unwrap(), 0);
        assert_eq!(try_f64_to_u32(u32::MAX as f64).unwrap(), u32::MAX);
    }

    #[test]
    fn test_errors() {
        let input = r##"name{registry="default" content_type="html"} 1890"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: ',', .. }, ..
            }
        ));

        let input = r##"# TYPE a counte"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::InvalidMetricKind { .. }, ..
            }
        ));

        let input = r##"# TYPEabcd asdf"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedSpace { .. }, ..
            }
        ));

        let input = r##"name{registry="} 1890"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: '"', .. }, ..
            }
        ));

        let input = r##"name{registry=} 1890"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: '"', .. }, ..
            }
        ));

        let input = r##"name abcd"##;
        let error = group_text_metrics(input).unwrap_err();
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ParseFloatError { .. }, ..
            }
        ));
    }
}

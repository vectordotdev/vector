use snafu::ResultExt;
use std::collections::BTreeMap;

mod line;

pub use line::ErrorKind;
use line::Line;
use line::Metric;
use line::MetricKind;

#[derive(Debug, snafu::Snafu, PartialEq)]
pub enum ParserError {
    #[snafu(display("{}, line: {:?}", kind, line))]
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

    #[snafu(display("invalid name {:?} for metric group {:?}", metric_name, group_name))]
    InvalidName {
        group_name: String,
        metric_name: String,
    },
}

#[derive(Debug, PartialEq)]
pub struct SummaryMetric {
    pub labels: BTreeMap<String, String>,
    pub value: SummaryMetricValue,
}

#[derive(Debug, PartialEq)]
pub enum SummaryMetricValue {
    Quantile { quantile: f64, value: f64 },
    Sum { sum: f64 },
    Count { count: f64 },
}

#[derive(Debug, PartialEq)]
pub struct HistogramMetric {
    pub labels: BTreeMap<String, String>,
    pub value: HistogramMetricValue,
}

#[derive(Debug, PartialEq)]
pub enum HistogramMetricValue {
    Bucket { bucket: f64, value: f64 },
    Sum { sum: f64 },
    Count { count: f64 },
}

#[derive(Debug, PartialEq)]
pub struct OtherMetric {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
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

    /// Check if the name belongs to this metric.
    fn check_name(&self, name: &str) -> bool {
        if !name.starts_with(&self.name) {
            return false;
        }
        let left = &name[self.name.len()..];
        match self.metrics {
            GroupKind::Histogram(_) => left == "_bucket" || left == "_sum" || left == "_count",
            GroupKind::Summary(_) => left.is_empty() || left == "_sum" || left == "_count",
            _ => left.is_empty(),
        }
    }

    fn push(&mut self, metric: Metric) -> Result<(), ParserError> {
        // this is an assertion
        if !self.check_name(&metric.name) {
            return Err(ParserError::InvalidName {
                group_name: self.name.clone(),
                metric_name: metric.name,
            });
        }

        match self.metrics {
            GroupKind::Counter(ref mut vec)
            | GroupKind::Gauge(ref mut vec)
            | GroupKind::Untyped(ref mut vec) => {
                vec.push(OtherMetric {
                    labels: metric.labels,
                    value: metric.value,
                });
            }
            GroupKind::Histogram(ref mut vec) => {
                let suffix = &metric.name[self.name.len()..];
                match suffix {
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
                                value: metric.value,
                            },
                        });
                    }
                    "_sum" => vec.push(HistogramMetric {
                        value: HistogramMetricValue::Sum { sum: metric.value },
                        labels: metric.labels,
                    }),
                    "_count" => vec.push(HistogramMetric {
                        value: HistogramMetricValue::Count {
                            count: metric.value,
                        },
                        labels: metric.labels,
                    }),
                    _ => unreachable!(),
                }
            }
            GroupKind::Summary(ref mut vec) => {
                let suffix = &metric.name[self.name.len()..];
                match suffix {
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
                        });
                    }
                    "_sum" => vec.push(SummaryMetric {
                        value: SummaryMetricValue::Sum { sum: metric.value },
                        labels: metric.labels,
                    }),
                    "_count" => vec.push(SummaryMetric {
                        value: SummaryMetricValue::Count {
                            count: metric.value,
                        },
                        labels: metric.labels,
                    }),
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }
}

pub fn group_metrics(input: &str) -> Result<Vec<MetricGroup>, ParserError> {
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
                    if groups.last().is_none() || !groups.last().unwrap().check_name(&metric.name) {
                        groups.push(MetricGroup::new(metric.name.clone(), MetricKind::Untyped));
                    }
                    groups.last_mut().unwrap().push(metric)?;
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
    fn test_group_metrics() {
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
        group_metrics(input).unwrap();
    }

    #[test]
    fn test_errors() {
        let input = r##"name{registry="default" content_type="html"} 1890"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: ',', .. }, ..
            }
        ));

        let input = r##"# TYPE a counte"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::InvalidMetricKind { .. }, ..
            }
        ));

        let input = r##"# TYPEabcd asdf"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedSpace { .. }, ..
            }
        ));

        let input = r##"name{registry="} 1890"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: '"', .. }, ..
            }
        ));

        let input = r##"name{registry=} 1890"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ExpectedChar { expected: '"', .. }, ..
            }
        ));

        let input = r##"name abcd"##;
        let error = group_metrics(input).unwrap_err();
        println!("{}", error);
        assert!(matches!(
            error,
            ParserError::WithLine {
                kind: ErrorKind::ParseFloatError { .. }, ..
            }
        ));
    }
}

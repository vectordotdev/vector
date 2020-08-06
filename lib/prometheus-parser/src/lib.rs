use std::collections::BTreeMap;

mod line;

#[derive(Debug, snafu::Snafu, PartialEq)]
pub enum ParserError {
    #[snafu(display("Expected \"le\" tag for histogram metric"))]
    ExpectedLeTag,
    #[snafu(display("Expected \"quantile\" tag for summary metric"))]
    ExpectedQuantileTag,
    #[snafu(display("Invalid metric type, input: {:?}", input))]
    InvalidMetricKind { input: String },
    #[snafu(display("Expected token {:?}, input: {:?}", expected, input))]
    ExpectedToken {
        expected: &'static str,
        input: String,
    },
    #[snafu(display("Name must start with [a-zA-Z_], input: {:?}", input))]
    ParseNameError { input: String },
    #[snafu(display("Parse float value error, input: {:?}", input))]
    ParseFloatError { input: String },

    // Error that we didn't catch
    #[snafu(display("Nom error {:?}, input: {:?}", kind, input))]
    Nom {
        input: String,
        kind: nom::error::ErrorKind,
    },

    // Below are bugs
    #[snafu(display("Nom return incomplete"))]
    NomIncomplete,
    #[snafu(display("Nom return failure"))]
    NomFailure,
    #[snafu(display("Invalid name {:?} for metric group {:?}", metric_name, group_name))]
    InvalidName {
        group_name: String,
        metric_name: String,
    },
}

impl From<ParserError> for nom::Err<ParserError> {
    fn from(error: ParserError) -> Self {
        nom::Err::Error(error)
    }
}

impl From<nom::Err<ParserError>> for ParserError {
    fn from(error: nom::Err<ParserError>) -> Self {
        match error {
            nom::Err::Incomplete(_) => ParserError::NomIncomplete,
            nom::Err::Failure(_) => ParserError::NomFailure,
            nom::Err::Error(e) => e,
        }
    }
}

impl<'a> nom::error::ParseError<&'a str> for ParserError {
    fn from_error_kind(input: &str, kind: nom::error::ErrorKind) -> Self {
        ParserError::Nom {
            input: input.to_owned(),
            kind,
        }
    }

    fn append(_: &str, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

use line::Line;
use line::Metric;
use line::MetricKind;

#[derive(Debug, PartialEq)]
pub enum SummaryMetric {
    Quantile {
        quantile: f64,
        value: f64,
        labels: BTreeMap<String, String>,
    },
    Sum {
        value: f64,
        labels: BTreeMap<String, String>,
    },
    Count {
        value: f64,
        labels: BTreeMap<String, String>,
    },
}

#[derive(Debug, PartialEq)]
pub enum HistogramMetric {
    Bucket {
        bucket: f64,
        value: f64,
        labels: BTreeMap<String, String>,
    },
    Sum {
        value: f64,
        labels: BTreeMap<String, String>,
    },
    Count {
        value: f64,
        labels: BTreeMap<String, String>,
    },
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

impl HistogramMetric {
    pub fn get_labels(&self) -> &BTreeMap<String, String> {
        match self {
            HistogramMetric::Bucket { labels, .. }
            | HistogramMetric::Count { labels, .. }
            | HistogramMetric::Sum { labels, .. } => labels,
        }
    }
}

impl SummaryMetric {
    pub fn get_labels(&self) -> &BTreeMap<String, String> {
        match self {
            SummaryMetric::Quantile { labels, .. }
            | SummaryMetric::Count { labels, .. }
            | SummaryMetric::Sum { labels, .. } => labels,
        }
    }
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

    fn push(&mut self, mut metric: Metric) -> Result<(), ParserError> {
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
                        let bucket = metric
                            .labels
                            .remove("le")
                            .ok_or(ParserError::ExpectedLeTag)?;
                        let (_, bucket) = line::Metric::parse_value(&bucket)?;
                        vec.push(HistogramMetric::Bucket {
                            bucket,
                            value: metric.value,
                            labels: metric.labels,
                        });
                    }
                    "_sum" => vec.push(HistogramMetric::Sum {
                        value: metric.value,
                        labels: metric.labels,
                    }),
                    "_count" => vec.push(HistogramMetric::Count {
                        value: metric.value,
                        labels: metric.labels,
                    }),
                    _ => unreachable!(),
                }
            }
            GroupKind::Summary(ref mut vec) => {
                let suffix = &metric.name[self.name.len()..];
                match suffix {
                    "" => {
                        let quantile = metric
                            .labels
                            .remove("quantile")
                            .ok_or(ParserError::ExpectedQuantileTag)?;
                        let (_, quantile) = line::Metric::parse_value(&quantile)?;
                        vec.push(SummaryMetric::Quantile {
                            quantile,
                            value: metric.value,
                            labels: metric.labels,
                        });
                    }
                    "_sum" => vec.push(SummaryMetric::Sum {
                        value: metric.value,
                        labels: metric.labels,
                    }),
                    "_count" => vec.push(SummaryMetric::Count {
                        value: metric.value,
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
        let line = Line::parse(line)?;
        if line.is_none() {
            continue;
        }
        let line = line.unwrap();
        match line {
            Line::Header(header) => {
                groups.push(MetricGroup::new(header.metric_name, header.kind));
            }
            Line::Metric(metric) => {
                let group = {
                    if groups.last().is_none() || !groups.last().unwrap().check_name(&metric.name) {
                        groups.push(MetricGroup::new(metric.name.clone(), MetricKind::Untyped));
                    }
                    groups.last_mut().unwrap()
                };
                group.push(metric)?;
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
}

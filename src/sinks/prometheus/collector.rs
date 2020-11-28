use crate::{
    event::metric::{Metric, MetricValue, StatisticKind},
    prometheus::{proto, METRIC_NAME_LABEL},
    sinks::util::{encode_namespace, statistic::DistributionStatistic},
};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

pub(super) trait MetricCollector {
    fn new() -> Self;

    fn emit(
        &mut self,
        timestamp_millis: i64,
        name: &str,
        suffix: &str,
        value: f64,
        tags: &Option<BTreeMap<String, String>>,
        extra: Option<(&str, String)>,
    );

    fn encode_metric(
        &mut self,
        default_namespace: Option<&str>,
        buckets: &[f64],
        quantiles: &[f64],
        expired: bool,
        metric: &Metric,
    ) {
        let name = encode_namespace(
            metric.namespace.as_deref().or(default_namespace),
            '_',
            &metric.name,
        );
        let name = &name;
        let timestamp = metric.timestamp.map(|t| t.timestamp_millis()).unwrap_or(0);

        if metric.kind.is_absolute() {
            let tags = &metric.tags;

            match &metric.value {
                MetricValue::Counter { value } => {
                    self.emit(timestamp, &name, "", *value, tags, None);
                }
                MetricValue::Gauge { value } => {
                    self.emit(timestamp, &name, "", *value, tags, None);
                }
                MetricValue::Set { values } => {
                    // sets could expire
                    let value = if expired { 0 } else { values.len() };
                    self.emit(timestamp, &name, "", value as f64, tags, None);
                }
                MetricValue::Distribution {
                    values,
                    sample_rates,
                    statistic: StatisticKind::Histogram,
                } => {
                    // convert distributions into aggregated histograms
                    let mut counts = vec![0; buckets.len()];
                    let mut sum = 0.0;
                    let mut count = 0;
                    for (v, c) in values.iter().zip(sample_rates.iter()) {
                        buckets
                            .iter()
                            .enumerate()
                            .skip_while(|&(_, b)| b < v)
                            .for_each(|(i, _)| {
                                counts[i] += c;
                            });

                        sum += v * (*c as f64);
                        count += c;
                    }

                    for (b, c) in buckets.iter().zip(counts.iter()) {
                        self.emit(
                            timestamp,
                            &name,
                            "_bucket",
                            *c as f64,
                            tags,
                            Some(("le", b.to_string())),
                        );
                    }
                    self.emit(
                        timestamp,
                        &name,
                        "_bucket",
                        count as f64,
                        tags,
                        Some(("le", "+Inf".to_string())),
                    );
                    self.emit(timestamp, &name, "_sum", sum as f64, tags, None);
                    self.emit(timestamp, &name, "_count", count as f64, tags, None);
                }
                MetricValue::Distribution {
                    values,
                    sample_rates,
                    statistic: StatisticKind::Summary,
                } => {
                    if let Some(statistic) =
                        DistributionStatistic::new(values, sample_rates, quantiles)
                    {
                        for (q, v) in statistic.quantiles.iter() {
                            self.emit(
                                timestamp,
                                &name,
                                "",
                                *v,
                                tags,
                                Some(("quantile", q.to_string())),
                            );
                        }
                        self.emit(timestamp, &name, "_sum", statistic.sum, tags, None);
                        self.emit(
                            timestamp,
                            &name,
                            "_count",
                            statistic.count as f64,
                            tags,
                            None,
                        );
                        self.emit(timestamp, &name, "_min", statistic.min, tags, None);
                        self.emit(timestamp, &name, "_max", statistic.max, tags, None);
                        self.emit(timestamp, &name, "_avg", statistic.avg, tags, None);
                    } else {
                        self.emit(timestamp, &name, "_sum", 0.0, tags, None);
                        self.emit(timestamp, &name, "_count", 0.0, tags, None);
                    }
                }
                MetricValue::AggregatedHistogram {
                    buckets,
                    counts,
                    count,
                    sum,
                } => {
                    let mut value = 0f64;
                    for (b, c) in buckets.iter().zip(counts.iter()) {
                        // prometheus uses cumulative histogram
                        // https://prometheus.io/docs/concepts/metric_types/#histogram
                        value += *c as f64;
                        self.emit(
                            timestamp,
                            &name,
                            "_bucket",
                            value,
                            tags,
                            Some(("le", b.to_string())),
                        );
                    }
                    self.emit(
                        timestamp,
                        &name,
                        "_bucket",
                        *count as f64,
                        tags,
                        Some(("le", "+Inf".to_string())),
                    );
                    self.emit(timestamp, &name, "_sum", *sum, tags, None);
                    self.emit(timestamp, &name, "_count", *count as f64, tags, None);
                }
                MetricValue::AggregatedSummary {
                    quantiles,
                    values,
                    count,
                    sum,
                } => {
                    for (q, v) in quantiles.iter().zip(values.iter()) {
                        self.emit(
                            timestamp,
                            &name,
                            "",
                            *v,
                            tags,
                            Some(("quantile", q.to_string())),
                        );
                    }
                    self.emit(timestamp, &name, "_sum", *sum, tags, None);
                    self.emit(timestamp, &name, "_count", *count as f64, tags, None);
                }
            }
        }
    }
}

pub(super) struct StringCollector {
    pub result: String,
}

impl MetricCollector for StringCollector {
    fn new() -> Self {
        let result = String::new();
        Self { result }
    }

    fn emit(
        &mut self,
        _timestamp_millis: i64,
        name: &str,
        suffix: &str,
        value: f64,
        tags: &Option<BTreeMap<String, String>>,
        extra: Option<(&str, String)>,
    ) {
        self.result.push_str(name);
        self.result.push_str(suffix);
        self.encode_tags(tags, extra);
        writeln!(&mut self.result, " {}", value).ok();
    }
}

impl StringCollector {
    fn encode_tags(
        &mut self,
        tags: &Option<BTreeMap<String, String>>,
        extra: Option<(&str, String)>,
    ) {
        match (tags, extra) {
            (None, None) => Ok(()),
            (None, Some(tag)) => write!(&mut self.result, "{{{}=\"{}\"}}", tag.0, tag.1),
            (Some(tags), ref tag) => {
                let mut parts = tags
                    .iter()
                    .map(|(name, value)| format!("{}=\"{}\"", name, value))
                    .collect::<Vec<_>>();

                if let Some(tag) = tag {
                    parts.push(format!("{}=\"{}\"", tag.0, tag.1));
                }

                parts.sort();
                write!(&mut self.result, "{{{}}}", parts.join(","))
            }
        }
        .ok();
    }

    pub(super) fn encode_header(&mut self, default_namespace: Option<&str>, metric: &Metric) {
        let name = &metric.name;
        let fullname =
            encode_namespace(metric.namespace.as_deref().or(default_namespace), '_', name);

        let r#type = match &metric.value {
            MetricValue::Counter { .. } => "counter",
            MetricValue::Gauge { .. } => "gauge",
            MetricValue::Distribution {
                statistic: StatisticKind::Histogram,
                ..
            } => "histogram",
            MetricValue::Distribution {
                statistic: StatisticKind::Summary,
                ..
            } => "summary",
            MetricValue::Set { .. } => "gauge",
            MetricValue::AggregatedHistogram { .. } => "histogram",
            MetricValue::AggregatedSummary { .. } => "summary",
        };

        writeln!(&mut self.result, "# HELP {} {}", fullname, name).ok();
        writeln!(&mut self.result, "# TYPE {} {}", fullname, r#type).ok();
    }
}

type Labels = Vec<proto::Label>;

pub(super) struct TimeSeries {
    buffer: HashMap<Labels, Vec<proto::Sample>>,
}

impl TimeSeries {
    fn make_labels(
        tags: &Option<BTreeMap<String, String>>,
        name: &str,
        suffix: &str,
        extra: Option<(&str, String)>,
    ) -> Labels {
        // Each Prometheus metric is grouped by its labels, which
        // contains all the labels from the source metric, plus the name
        // label for the actual metric name. For convenience below, an
        // optional extra tag is added.
        let mut labels = tags.clone().unwrap_or_default();
        labels.insert(METRIC_NAME_LABEL.into(), [name, suffix].join(""));
        if let Some((name, value)) = extra {
            labels.insert(name.into(), value);
        }

        // Extract the labels into a vec and sort to produce a
        // consistent key for the buffer.
        let mut labels = labels
            .into_iter()
            .map(|(name, value)| proto::Label { name, value })
            .collect::<Labels>();
        labels.sort();
        labels
    }

    pub(super) fn finish(self) -> Vec<proto::TimeSeries> {
        self.buffer
            .into_iter()
            .map(|(labels, samples)| proto::TimeSeries { labels, samples })
            .collect()
    }
}

impl MetricCollector for TimeSeries {
    fn new() -> Self {
        Self {
            buffer: Default::default(),
        }
    }

    fn emit(
        &mut self,
        timestamp_millis: i64,
        name: &str,
        suffix: &str,
        value: f64,
        tags: &Option<BTreeMap<String, String>>,
        extra: Option<(&str, String)>,
    ) {
        self.buffer
            .entry(Self::make_labels(tags, name, suffix, extra))
            .or_default()
            .push(proto::Sample {
                value,
                timestamp: timestamp_millis,
            });
    }
}

#[cfg(test)]
mod tests {
    use super::super::default_summary_quantiles;
    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};
    use pretty_assertions::assert_eq;

    fn encode_metric_header(default_namespace: Option<&str>, metric: &Metric) -> String {
        let mut s = StringCollector::new();
        s.encode_header(default_namespace, metric);
        s.result
    }

    fn encode_metric_datum(
        default_namespace: Option<&str>,
        buckets: &[f64],
        quantiles: &[f64],
        expired: bool,
        metric: &Metric,
    ) -> String {
        let mut s = StringCollector::new();
        s.encode_metric(default_namespace, buckets, quantiles, expired, metric);
        s.result
    }

    fn tags() -> BTreeMap<String, String> {
        vec![("code".to_owned(), "200".to_owned())]
            .into_iter()
            .collect()
    }

    #[test]
    fn test_encode_counter() {
        let metric = Metric {
            name: "hits".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 10.0 },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP vector_hits hits\n# TYPE vector_hits counter\n".to_owned()
        );
        assert_eq!(frame, "vector_hits{code=\"200\"} 10\n".to_owned());
    }

    #[test]
    fn test_encode_gauge() {
        let metric = Metric {
            name: "temperature".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: -1.1 },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP vector_temperature temperature\n# TYPE vector_temperature gauge\n".to_owned()
        );
        assert_eq!(frame, "vector_temperature{code=\"200\"} -1.1\n".to_owned());
    }

    #[test]
    fn test_encode_set() {
        let metric = Metric {
            name: "users".to_owned(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Set {
                values: vec!["foo".into()].into_iter().collect(),
            },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP vector_users users\n# TYPE vector_users gauge\n".to_owned()
        );
        assert_eq!(frame, "vector_users 1\n".to_owned());
    }

    #[test]
    fn test_encode_expired_set() {
        let metric = Metric {
            name: "users".to_owned(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Set {
                values: vec!["foo".into()].into_iter().collect(),
            },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[], &[], true, &metric);

        assert_eq!(
            header,
            "# HELP vector_users users\n# TYPE vector_users gauge\n".to_owned()
        );
        assert_eq!(frame, "vector_users 0\n".to_owned());
    }

    #[test]
    fn test_encode_distribution() {
        let metric = Metric {
            name: "requests".to_owned(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0, 3.0],
                sample_rates: vec![3, 3, 2],
                statistic: StatisticKind::Histogram,
            },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[0.0, 2.5, 5.0], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP vector_requests requests\n# TYPE vector_requests histogram\n".to_owned()
        );
        assert_eq!(frame, "vector_requests_bucket{le=\"0\"} 0\nvector_requests_bucket{le=\"2.5\"} 6\nvector_requests_bucket{le=\"5\"} 8\nvector_requests_bucket{le=\"+Inf\"} 8\nvector_requests_sum 15\nvector_requests_count 8\n".to_owned());
    }

    #[test]
    fn test_encode_histogram() {
        let metric = Metric {
            name: "requests".to_owned(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedHistogram {
                buckets: vec![1.0, 2.1, 3.0],
                counts: vec![1, 2, 3],
                count: 6,
                sum: 12.5,
            },
        };

        let header = encode_metric_header(Some("vector"), &metric);
        let frame = encode_metric_datum(Some("vector"), &[], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP vector_requests requests\n# TYPE vector_requests histogram\n".to_owned()
        );
        assert_eq!(
            frame,
            r#"vector_requests_bucket{le="1"} 1
vector_requests_bucket{le="2.1"} 3
vector_requests_bucket{le="3"} 6
vector_requests_bucket{le="+Inf"} 6
vector_requests_sum 12.5
vector_requests_count 6
"#
        );
    }

    #[test]
    fn test_encode_summary() {
        let metric = Metric {
            name: "requests".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::AggregatedSummary {
                quantiles: vec![0.01, 0.5, 0.99],
                values: vec![1.5, 2.0, 3.0],
                count: 6,
                sum: 12.0,
            },
        };

        let header = encode_metric_header(Some("ns"), &metric);
        let frame = encode_metric_datum(Some("ns"), &[], &[], false, &metric);

        assert_eq!(
            header,
            "# HELP ns_requests requests\n# TYPE ns_requests summary\n".to_owned()
        );
        assert_eq!(frame, "ns_requests{code=\"200\",quantile=\"0.01\"} 1.5\nns_requests{code=\"200\",quantile=\"0.5\"} 2\nns_requests{code=\"200\",quantile=\"0.99\"} 3\nns_requests_sum{code=\"200\"} 12\nns_requests_count{code=\"200\"} 6\n".to_owned());
    }

    #[test]
    fn test_encode_distribution_summary() {
        let metric = Metric {
            name: "requests".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Distribution {
                values: vec![1.0, 2.0, 3.0],
                sample_rates: vec![3, 3, 2],
                statistic: StatisticKind::Summary,
            },
        };

        let header = encode_metric_header(Some("ns"), &metric);
        let frame = encode_metric_datum(
            Some("ns"),
            &[],
            &default_summary_quantiles(),
            false,
            &metric,
        );

        assert_eq!(
            header,
            "# HELP ns_requests requests\n# TYPE ns_requests summary\n".to_owned()
        );
        assert_eq!(frame, "ns_requests{code=\"200\",quantile=\"0.5\"} 2\nns_requests{code=\"200\",quantile=\"0.75\"} 2\nns_requests{code=\"200\",quantile=\"0.9\"} 3\nns_requests{code=\"200\",quantile=\"0.95\"} 3\nns_requests{code=\"200\",quantile=\"0.99\"} 3\nns_requests_sum{code=\"200\"} 15\nns_requests_count{code=\"200\"} 8\nns_requests_min{code=\"200\"} 1\nns_requests_max{code=\"200\"} 3\nns_requests_avg{code=\"200\"} 1.875\n".to_owned());
    }
}

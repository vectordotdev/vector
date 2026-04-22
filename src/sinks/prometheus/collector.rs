use std::{collections::BTreeMap, fmt::Write as _};

use chrono::Utc;
use indexmap::map::IndexMap;
use vector_lib::{
    event::metric::{
        MetricSketch, MetricTags, NativeHistogramBuckets, NativeHistogramCount,
        NativeHistogramResetHint, NativeHistogramSpan, Quantile, samples_to_buckets,
    },
    prometheus::parser::{METRIC_NAME_LABEL, proto},
};

use crate::{
    event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
    sinks::util::{encode_namespace, statistic::DistributionStatistic},
};

pub(super) trait MetricCollector {
    type Output;

    fn new() -> Self;

    fn emit_metadata(&mut self, name: &str, fullname: &str, value: &MetricValue);

    fn emit_value(
        &mut self,
        timestamp_millis: Option<i64>,
        name: &str,
        suffix: &str,
        value: f64,
        tags: Option<&MetricTags>,
        extra: Option<(&str, String)>,
    );

    /// Emit a native histogram.
    ///
    /// The default implementation converts the native histogram to a classic aggregated histogram
    /// and emits it as multiple samples. Collectors that support native histograms (e.g.,
    /// Prometheus remote write) should override this to emit a proper native histogram.
    #[allow(clippy::too_many_arguments)]
    fn emit_native_histogram(
        &mut self,
        timestamp_millis: Option<i64>,
        name: &str,
        tags: Option<&MetricTags>,
        count: &NativeHistogramCount,
        sum: f64,
        schema: i32,
        zero_threshold: f64,
        zero_count: &NativeHistogramCount,
        positive_spans: &[NativeHistogramSpan],
        positive_buckets: &NativeHistogramBuckets,
        negative_spans: &[NativeHistogramSpan],
        negative_buckets: &NativeHistogramBuckets,
        reset_hint: NativeHistogramResetHint,
    ) {
        // Fallback: build a classic histogram representation and emit it as float samples.
        // This is lossy but allows the text exposition format to represent native histograms.
        let native_value = MetricValue::NativeHistogram {
            count: *count,
            sum,
            schema,
            zero_threshold,
            zero_count: *zero_count,
            positive_spans: positive_spans.to_vec(),
            positive_buckets: positive_buckets.clone(),
            negative_spans: negative_spans.to_vec(),
            negative_buckets: negative_buckets.clone(),
            reset_hint,
        };
        if let Some(MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        }) = native_value.native_histogram_to_agg_histogram()
        {
            let mut bucket_count = 0.0;
            for bucket in &buckets {
                if bucket.upper_limit.is_infinite() {
                    continue;
                }
                bucket_count += bucket.count as f64;
                self.emit_value(
                    timestamp_millis,
                    name,
                    "_bucket",
                    bucket_count,
                    tags,
                    Some(("le", bucket.upper_limit.to_string())),
                );
            }
            self.emit_value(
                timestamp_millis,
                name,
                "_bucket",
                count as f64,
                tags,
                Some(("le", "+Inf".to_string())),
            );
            self.emit_value(timestamp_millis, name, "_sum", sum, tags, None);
            self.emit_value(timestamp_millis, name, "_count", count as f64, tags, None);
        }
    }

    fn finish(self) -> Self::Output;

    fn encode_metric(
        &mut self,
        default_namespace: Option<&str>,
        buckets: &[f64],
        quantiles: &[f64],
        metric: &Metric,
    ) {
        let name = encode_namespace(metric.namespace().or(default_namespace), '_', metric.name());
        let name = &name;
        let timestamp = metric.timestamp().map(|t| t.timestamp_millis());

        if metric.kind() == MetricKind::Absolute {
            let tags = metric.tags();
            self.emit_metadata(metric.name(), name, metric.value());

            match metric.value() {
                MetricValue::Counter { value } => {
                    self.emit_value(timestamp, name, "", *value, tags, None);
                }
                MetricValue::Gauge { value } => {
                    self.emit_value(timestamp, name, "", *value, tags, None);
                }
                MetricValue::Set { values } => {
                    self.emit_value(timestamp, name, "", values.len() as f64, tags, None);
                }
                MetricValue::Distribution {
                    samples,
                    statistic: StatisticKind::Histogram,
                } => {
                    // convert distributions into aggregated histograms
                    let (buckets, count, sum) = samples_to_buckets(samples, buckets);
                    let mut bucket_count = 0.0;
                    for bucket in buckets {
                        bucket_count += bucket.count as f64;
                        self.emit_value(
                            timestamp,
                            name,
                            "_bucket",
                            bucket_count,
                            tags,
                            Some(("le", bucket.upper_limit.to_string())),
                        );
                    }
                    self.emit_value(
                        timestamp,
                        name,
                        "_bucket",
                        count as f64,
                        tags,
                        Some(("le", "+Inf".to_string())),
                    );
                    self.emit_value(timestamp, name, "_sum", sum, tags, None);
                    self.emit_value(timestamp, name, "_count", count as f64, tags, None);
                }
                MetricValue::Distribution {
                    samples,
                    statistic: StatisticKind::Summary,
                } => {
                    if let Some(statistic) = DistributionStatistic::from_samples(samples, quantiles)
                    {
                        for (q, v) in statistic.quantiles.iter() {
                            self.emit_value(
                                timestamp,
                                name,
                                "",
                                *v,
                                tags,
                                Some(("quantile", q.to_string())),
                            );
                        }
                        self.emit_value(timestamp, name, "_sum", statistic.sum, tags, None);
                        self.emit_value(
                            timestamp,
                            name,
                            "_count",
                            statistic.count as f64,
                            tags,
                            None,
                        );
                        self.emit_value(timestamp, name, "_min", statistic.min, tags, None);
                        self.emit_value(timestamp, name, "_max", statistic.max, tags, None);
                        self.emit_value(timestamp, name, "_avg", statistic.avg, tags, None);
                    } else {
                        self.emit_value(timestamp, name, "_sum", 0.0, tags, None);
                        self.emit_value(timestamp, name, "_count", 0.0, tags, None);
                    }
                }
                MetricValue::AggregatedHistogram {
                    buckets,
                    count,
                    sum,
                } => {
                    let mut bucket_count = 0.0;
                    for bucket in buckets {
                        // Aggregated histograms are cumulative in Prometheus.  This means that the
                        // count of values in a bucket should only go up at the upper limit goes up,
                        // because if you count a value in a specific bucket, by definition, it is
                        // less than the upper limit of the next bucket.
                        //
                        // While most sources should give us buckets that have an "infinity" bucket
                        // -- everything else that didn't fit in the non-infinity-upper-limit buckets
                        // -- we can't be sure, so we calculate that bucket ourselves.  This is why
                        // we make sure to avoid encoding a bucket if its upper limit is already
                        // infinity, so that we don't double report.
                        //
                        // This check will also avoid printing out a bucket whose upper limit is
                        // negative infinity, because that would make no sense.
                        if bucket.upper_limit.is_infinite() {
                            continue;
                        }

                        bucket_count += bucket.count as f64;
                        self.emit_value(
                            timestamp,
                            name,
                            "_bucket",
                            bucket_count,
                            tags,
                            Some(("le", bucket.upper_limit.to_string())),
                        );
                    }
                    self.emit_value(
                        timestamp,
                        name,
                        "_bucket",
                        *count as f64,
                        tags,
                        Some(("le", "+Inf".to_string())),
                    );
                    self.emit_value(timestamp, name, "_sum", *sum, tags, None);
                    self.emit_value(timestamp, name, "_count", *count as f64, tags, None);
                }
                MetricValue::AggregatedSummary {
                    quantiles,
                    count,
                    sum,
                } => {
                    for quantile in quantiles {
                        self.emit_value(
                            timestamp,
                            name,
                            "",
                            quantile.value,
                            tags,
                            Some(("quantile", quantile.quantile.to_string())),
                        );
                    }
                    self.emit_value(timestamp, name, "_sum", *sum, tags, None);
                    self.emit_value(timestamp, name, "_count", *count as f64, tags, None);
                }
                MetricValue::Sketch { sketch } => match sketch {
                    MetricSketch::AgentDDSketch(ddsketch) => {
                        for q in quantiles {
                            let quantile = Quantile {
                                quantile: *q,
                                value: ddsketch.quantile(*q).unwrap_or(0.0),
                            };
                            self.emit_value(
                                timestamp,
                                name,
                                "",
                                quantile.value,
                                tags,
                                Some(("quantile", quantile.quantile.to_string())),
                            );
                        }
                        self.emit_value(
                            timestamp,
                            name,
                            "_sum",
                            ddsketch.sum().unwrap_or(0.0),
                            tags,
                            None,
                        );
                        self.emit_value(
                            timestamp,
                            name,
                            "_count",
                            ddsketch.count() as f64,
                            tags,
                            None,
                        );
                    }
                },
                MetricValue::NativeHistogram {
                    count,
                    sum,
                    schema,
                    zero_threshold,
                    zero_count,
                    positive_spans,
                    positive_buckets,
                    negative_spans,
                    negative_buckets,
                    reset_hint,
                } => {
                    self.emit_native_histogram(
                        timestamp,
                        name,
                        tags,
                        count,
                        *sum,
                        *schema,
                        *zero_threshold,
                        zero_count,
                        positive_spans,
                        positive_buckets,
                        negative_spans,
                        negative_buckets,
                        *reset_hint,
                    );
                }
            }
        }
    }
}

pub(super) struct StringCollector {
    // BTreeMap ensures we get sorted output, which whilst not required is preferable
    processed: BTreeMap<String, String>,
}

impl MetricCollector for StringCollector {
    type Output = String;

    fn new() -> Self {
        let processed = BTreeMap::new();
        Self { processed }
    }

    fn emit_metadata(&mut self, name: &str, fullname: &str, value: &MetricValue) {
        if !self.processed.contains_key(fullname) {
            let header = Self::encode_header(name, fullname, value);
            self.processed.insert(fullname.into(), header);
        }
    }

    fn emit_value(
        &mut self,
        timestamp_millis: Option<i64>,
        name: &str,
        suffix: &str,
        value: f64,
        tags: Option<&MetricTags>,
        extra: Option<(&str, String)>,
    ) {
        let result = self
            .processed
            .get_mut(name)
            .expect("metric metadata not encoded");

        result.push_str(name);
        result.push_str(suffix);
        Self::encode_tags(result, tags, extra);
        _ = match timestamp_millis {
            None => writeln!(result, " {value}"),
            Some(timestamp) => writeln!(result, " {value} {timestamp}"),
        };
    }

    fn finish(self) -> String {
        self.processed.into_values().collect()
    }
}

impl StringCollector {
    fn encode_tags(result: &mut String, tags: Option<&MetricTags>, extra: Option<(&str, String)>) {
        match (tags, extra) {
            (None, None) => Ok(()),
            (None, Some(tag)) => write!(result, "{{{}}}", Self::format_tag(tag.0, &tag.1)),
            (Some(tags), ref tag) => {
                let mut parts = tags
                    .iter_single()
                    .map(|(key, value)| Self::format_tag(key, value))
                    .collect::<Vec<_>>();

                if let Some((key, value)) = tag {
                    parts.push(Self::format_tag(key, value))
                }

                parts.sort();
                write!(result, "{{{}}}", parts.join(","))
            }
        }
        .ok();
    }

    fn encode_header(name: &str, fullname: &str, value: &MetricValue) -> String {
        let r#type = prometheus_metric_type(value).as_str();
        format!("# HELP {fullname} {name}\n# TYPE {fullname} {type}\n")
    }

    fn format_tag(key: &str, mut value: &str) -> String {
        // For most tags, this is just `{KEY}="{VALUE}"` so allocate optimistically
        let mut result = String::with_capacity(key.len() + value.len() + 3);
        result.push_str(key);
        result.push_str("=\"");
        while let Some(i) = value.find(['\\', '"']) {
            result.push_str(&value[..i]);
            result.push('\\');
            // Ugly but works because we know the character at `i` is ASCII
            result.push(value.as_bytes()[i] as char);
            value = &value[i + 1..];
        }
        result.push_str(value);
        result.push('"');
        result
    }
}

type Labels = Vec<proto::Label>;

#[derive(Default)]
struct SeriesEntry {
    samples: Vec<proto::Sample>,
    histograms: Vec<proto::Histogram>,
}

pub(super) struct TimeSeries {
    buffer: IndexMap<Labels, SeriesEntry>,
    metadata: IndexMap<String, proto::MetricMetadata>,
    timestamp: Option<i64>,
}

impl TimeSeries {
    fn make_labels(
        tags: Option<&MetricTags>,
        name: &str,
        suffix: &str,
        extra: Option<(&str, String)>,
    ) -> Labels {
        // Each Prometheus metric is grouped by its labels, which
        // contains all the labels from the source metric, plus the name
        // label for the actual metric name. For convenience below, an
        // optional extra tag is added.
        let mut labels = tags.cloned().unwrap_or_default();
        labels.replace(METRIC_NAME_LABEL.into(), [name, suffix].join(""));
        if let Some((name, value)) = extra {
            labels.replace(name.into(), value);
        }

        // Extract the labels into a vec and sort to produce a
        // consistent key for the buffer.
        let mut labels = labels
            .into_iter_single()
            .map(|(name, value)| proto::Label { name, value })
            .collect::<Labels>();
        labels.sort();
        labels
    }

    fn default_timestamp(&mut self) -> i64 {
        *self
            .timestamp
            .get_or_insert_with(|| Utc::now().timestamp_millis())
    }
}

impl MetricCollector for TimeSeries {
    type Output = proto::WriteRequest;

    fn new() -> Self {
        Self {
            buffer: Default::default(),
            metadata: Default::default(),
            timestamp: None,
        }
    }

    fn emit_metadata(&mut self, name: &str, fullname: &str, value: &MetricValue) {
        if !self.metadata.contains_key(name) {
            let r#type = prometheus_metric_type(value);
            let metadata = proto::MetricMetadata {
                r#type: r#type as i32,
                metric_family_name: fullname.into(),
                help: name.into(),
                unit: String::new(),
            };
            self.metadata.insert(name.into(), metadata);
        }
    }

    fn emit_value(
        &mut self,
        timestamp_millis: Option<i64>,
        name: &str,
        suffix: &str,
        value: f64,
        tags: Option<&MetricTags>,
        extra: Option<(&str, String)>,
    ) {
        let timestamp = timestamp_millis.unwrap_or_else(|| self.default_timestamp());
        self.buffer
            .entry(Self::make_labels(tags, name, suffix, extra))
            .or_default()
            .samples
            .push(proto::Sample { value, timestamp });
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_native_histogram(
        &mut self,
        timestamp_millis: Option<i64>,
        name: &str,
        tags: Option<&MetricTags>,
        count: &NativeHistogramCount,
        sum: f64,
        schema: i32,
        zero_threshold: f64,
        zero_count: &NativeHistogramCount,
        positive_spans: &[NativeHistogramSpan],
        positive_buckets: &NativeHistogramBuckets,
        negative_spans: &[NativeHistogramSpan],
        negative_buckets: &NativeHistogramBuckets,
        reset_hint: NativeHistogramResetHint,
    ) {
        use proto::histogram::{Count, ResetHint, ZeroCount};

        let timestamp = timestamp_millis.unwrap_or_else(|| self.default_timestamp());

        let proto_count = match count {
            NativeHistogramCount::Integer(v) => Count::CountInt(*v),
            NativeHistogramCount::Float(v) => Count::CountFloat(*v),
        };

        let proto_zero_count = match zero_count {
            NativeHistogramCount::Integer(v) => ZeroCount::ZeroCountInt(*v),
            NativeHistogramCount::Float(v) => ZeroCount::ZeroCountFloat(*v),
        };

        let proto_reset_hint = match reset_hint {
            NativeHistogramResetHint::Unknown => ResetHint::Unknown,
            NativeHistogramResetHint::Yes => ResetHint::Yes,
            NativeHistogramResetHint::No => ResetHint::No,
            NativeHistogramResetHint::Gauge => ResetHint::Gauge,
        };

        let (positive_deltas, positive_counts) = match positive_buckets {
            NativeHistogramBuckets::IntegerDeltas(d) => (d.clone(), Vec::new()),
            NativeHistogramBuckets::FloatCounts(c) => (Vec::new(), c.clone()),
        };

        let (negative_deltas, negative_counts) = match negative_buckets {
            NativeHistogramBuckets::IntegerDeltas(d) => (d.clone(), Vec::new()),
            NativeHistogramBuckets::FloatCounts(c) => (Vec::new(), c.clone()),
        };

        let histogram = proto::Histogram {
            count: Some(proto_count),
            sum,
            schema,
            zero_threshold,
            zero_count: Some(proto_zero_count),
            negative_spans: negative_spans
                .iter()
                .map(|s| proto::BucketSpan {
                    offset: s.offset,
                    length: s.length,
                })
                .collect(),
            negative_deltas,
            negative_counts,
            positive_spans: positive_spans
                .iter()
                .map(|s| proto::BucketSpan {
                    offset: s.offset,
                    length: s.length,
                })
                .collect(),
            positive_deltas,
            positive_counts,
            reset_hint: proto_reset_hint as i32,
            timestamp,
        };

        self.buffer
            .entry(Self::make_labels(tags, name, "", None))
            .or_default()
            .histograms
            .push(histogram);
    }

    fn finish(self) -> proto::WriteRequest {
        let timeseries = self
            .buffer
            .into_iter()
            .map(|(labels, entry)| proto::TimeSeries {
                labels,
                samples: entry.samples,
                histograms: entry.histograms,
            })
            .collect::<Vec<_>>();
        let metadata = self
            .metadata
            .into_iter()
            .map(|(_, metadata)| metadata)
            .collect();
        proto::WriteRequest {
            timeseries,
            metadata,
        }
    }
}

const fn prometheus_metric_type(metric_value: &MetricValue) -> proto::MetricType {
    use proto::MetricType;
    match metric_value {
        MetricValue::Counter { .. } => MetricType::Counter,
        MetricValue::Gauge { .. } | MetricValue::Set { .. } => MetricType::Gauge,
        MetricValue::Distribution {
            statistic: StatisticKind::Histogram,
            ..
        } => MetricType::Histogram,
        MetricValue::Distribution {
            statistic: StatisticKind::Summary,
            ..
        } => MetricType::Summary,
        MetricValue::AggregatedHistogram { .. } => MetricType::Histogram,
        MetricValue::AggregatedSummary { .. } => MetricType::Summary,
        MetricValue::Sketch { .. } => MetricType::Summary,
        MetricValue::NativeHistogram { .. } => MetricType::Histogram,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{DateTime, TimeZone, Timelike};
    use indoc::indoc;
    use similar_asserts::assert_eq;
    use vector_lib::metric_tags;

    use super::{super::default_summary_quantiles, *};
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
        test_util::stats::VariableHistogram,
    };

    fn encode_one<T: MetricCollector>(
        default_namespace: Option<&str>,
        buckets: &[f64],
        quantiles: &[f64],
        metric: &Metric,
    ) -> T::Output {
        let mut s = T::new();
        s.encode_metric(default_namespace, buckets, quantiles, metric);
        s.finish()
    }

    fn tags() -> MetricTags {
        metric_tags!("code" => "200")
    }

    macro_rules! write_request {
        ( $name:literal, $help:literal, $type:ident
          [ $(
              $suffix:literal @ $timestamp:literal = $svalue:literal
                  [ $( $label:literal => $lvalue:literal ),* ]
          ),* ]
        ) => {
            proto::WriteRequest {
                timeseries: vec![
                    $(
                        proto::TimeSeries {
                            labels: vec![
                                proto::Label {
                                    name: "__name__".into(),
                                    value: format!("{}{}", $name, $suffix),
                                },
                                $(
                                    proto::Label {
                                        name: $label.into(),
                                        value: $lvalue.into(),
                                    },
                                )*
                            ],
                            samples: vec![ proto::Sample {
                                value: $svalue,
                                timestamp: $timestamp,
                            }],
                            histograms: vec![],
                        },
                    )*
                ],
                metadata: vec![proto::MetricMetadata {
                    r#type: proto::metric_metadata::MetricType::$type as i32,
                    metric_family_name: $name.into(),
                    help: $help.into(),
                    unit: "".into(),
                }],
            }
        };
    }

    #[test]
    fn encodes_counter_text() {
        assert_eq!(
            encode_counter::<StringCollector>(),
            indoc! { r#"
                # HELP vector_hits hits
                # TYPE vector_hits counter
                vector_hits{code="200"} 10 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_counter_request() {
        assert_eq!(
            encode_counter::<TimeSeries>(),
            write_request!("vector_hits", "hits", Counter ["" @ 1612325106789 = 10.0 ["code" => "200"]])
        );
    }

    fn encode_counter<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "hits".to_owned(),
            MetricKind::Absolute,
            MetricValue::Counter { value: 10.0 },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[], &[], &metric)
    }

    #[test]
    fn encodes_gauge_text() {
        assert_eq!(
            encode_gauge::<StringCollector>(),
            indoc! { r#"
                # HELP vector_temperature temperature
                # TYPE vector_temperature gauge
                vector_temperature{code="200"} -1.1 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_gauge_request() {
        assert_eq!(
            encode_gauge::<TimeSeries>(),
            write_request!("vector_temperature", "temperature", Gauge ["" @ 1612325106789 = -1.1 ["code" => "200"]])
        );
    }

    fn encode_gauge<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "temperature".to_owned(),
            MetricKind::Absolute,
            MetricValue::Gauge { value: -1.1 },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[], &[], &metric)
    }

    #[test]
    fn encodes_set_text() {
        assert_eq!(
            encode_set::<StringCollector>(),
            indoc! { r"
                # HELP vector_users users
                # TYPE vector_users gauge
                vector_users 1 1612325106789
            "}
        );
    }

    #[test]
    fn encodes_set_request() {
        assert_eq!(
            encode_set::<TimeSeries>(),
            write_request!("vector_users", "users", Gauge [ "" @ 1612325106789 = 1.0 []])
        );
    }

    fn encode_set<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "users".to_owned(),
            MetricKind::Absolute,
            MetricValue::Set {
                values: vec!["foo".into()].into_iter().collect(),
            },
        )
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[], &[], &metric)
    }

    #[test]
    fn encodes_expired_set_text() {
        assert_eq!(
            encode_expired_set::<StringCollector>(),
            indoc! {r"
                # HELP vector_users users
                # TYPE vector_users gauge
                vector_users 0 1612325106789
            "}
        );
    }

    #[test]
    fn encodes_expired_set_request() {
        assert_eq!(
            encode_expired_set::<TimeSeries>(),
            write_request!("vector_users", "users", Gauge ["" @ 1612325106789 = 0.0 []])
        );
    }

    fn encode_expired_set<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "users".to_owned(),
            MetricKind::Absolute,
            MetricValue::Set {
                values: BTreeSet::new(),
            },
        )
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[], &[], &metric)
    }

    #[test]
    fn encodes_distribution_text() {
        assert_eq!(
            encode_distribution::<StringCollector>(),
            indoc! {r#"
                # HELP vector_requests requests
                # TYPE vector_requests histogram
                vector_requests_bucket{le="0"} 0 1612325106789
                vector_requests_bucket{le="2.5"} 6 1612325106789
                vector_requests_bucket{le="5"} 8 1612325106789
                vector_requests_bucket{le="+Inf"} 8 1612325106789
                vector_requests_sum 15 1612325106789
                vector_requests_count 8 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_distribution_request() {
        assert_eq!(
            encode_distribution::<TimeSeries>(),
            write_request!(
                "vector_requests", "requests", Histogram [
                        "_bucket" @ 1612325106789 = 0.0 ["le" => "0"],
                        "_bucket" @ 1612325106789 = 6.0 ["le" => "2.5"],
                        "_bucket" @ 1612325106789 = 8.0 ["le" => "5"],
                        "_bucket" @ 1612325106789 = 8.0 ["le" => "+Inf"],
                        "_sum" @ 1612325106789 = 15.0 [],
                        "_count" @ 1612325106789 = 8.0 []
                ]
            )
        );
    }

    fn encode_distribution<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "requests".to_owned(),
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples: vector_lib::samples![1.0 => 3, 2.0 => 3, 3.0 => 2],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[0.0, 2.5, 5.0], &[], &metric)
    }

    #[test]
    fn encodes_histogram_text() {
        assert_eq!(
            encode_histogram::<StringCollector>(false),
            indoc! {r#"
                # HELP vector_requests requests
                # TYPE vector_requests histogram
                vector_requests_bucket{le="1"} 1 1612325106789
                vector_requests_bucket{le="2.1"} 3 1612325106789
                vector_requests_bucket{le="3"} 6 1612325106789
                vector_requests_bucket{le="+Inf"} 6 1612325106789
                vector_requests_sum 11.5 1612325106789
                vector_requests_count 6 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_histogram_request() {
        assert_eq!(
            encode_histogram::<TimeSeries>(false),
            write_request!(
                "vector_requests", "requests", Histogram [
                        "_bucket" @ 1612325106789 = 1.0 ["le" => "1"],
                        "_bucket" @ 1612325106789 = 3.0 ["le" => "2.1"],
                        "_bucket" @ 1612325106789 = 6.0 ["le" => "3"],
                        "_bucket" @ 1612325106789 = 6.0 ["le" => "+Inf"],
                        "_sum" @ 1612325106789 = 11.5 [],
                        "_count" @ 1612325106789 = 6.0 []
                    ]
            )
        );
    }

    #[test]
    fn encodes_histogram_text_with_extra_infinity_bound() {
        assert_eq!(
            encode_histogram::<StringCollector>(true),
            indoc! {r#"
                # HELP vector_requests requests
                # TYPE vector_requests histogram
                vector_requests_bucket{le="1"} 1 1612325106789
                vector_requests_bucket{le="2.1"} 3 1612325106789
                vector_requests_bucket{le="3"} 6 1612325106789
                vector_requests_bucket{le="+Inf"} 6 1612325106789
                vector_requests_sum 11.5 1612325106789
                vector_requests_count 6 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_histogram_request_with_extra_infinity_bound() {
        assert_eq!(
            encode_histogram::<TimeSeries>(true),
            write_request!(
                "vector_requests", "requests", Histogram [
                        "_bucket" @ 1612325106789 = 1.0 ["le" => "1"],
                        "_bucket" @ 1612325106789 = 3.0 ["le" => "2.1"],
                        "_bucket" @ 1612325106789 = 6.0 ["le" => "3"],
                        "_bucket" @ 1612325106789 = 6.0 ["le" => "+Inf"],
                        "_sum" @ 1612325106789 = 11.5 [],
                        "_count" @ 1612325106789 = 6.0 []
                    ]
            )
        );
    }

    fn encode_histogram<T: MetricCollector>(add_inf_bound: bool) -> T::Output {
        let bounds = if add_inf_bound {
            &[1.0, 2.1, 3.0, f64::INFINITY][..]
        } else {
            &[1.0, 2.1, 3.0][..]
        };

        let mut histogram = VariableHistogram::new(bounds);
        histogram.record_many(&[0.4, 2.0, 1.75, 2.6, 2.25, 2.5][..]);

        let metric = Metric::new(
            "requests".to_owned(),
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: histogram.buckets(),
                count: histogram.count(),
                sum: histogram.sum(),
            },
        )
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("vector"), &[], &[], &metric)
    }

    #[test]
    fn encodes_summary_text() {
        assert_eq!(
            encode_summary::<StringCollector>(),
            indoc! {r#"# HELP ns_requests requests
                # TYPE ns_requests summary
                ns_requests{code="200",quantile="0.01"} 1.5 1612325106789
                ns_requests{code="200",quantile="0.5"} 2 1612325106789
                ns_requests{code="200",quantile="0.99"} 3 1612325106789
                ns_requests_sum{code="200"} 12 1612325106789
                ns_requests_count{code="200"} 6 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_summary_request() {
        assert_eq!(
            encode_summary::<TimeSeries>(),
            write_request!(
                "ns_requests", "requests", Summary [
                    "" @ 1612325106789 = 1.5 ["code" => "200", "quantile" => "0.01"],
                    "" @ 1612325106789 = 2.0 ["code" => "200", "quantile" => "0.5"],
                    "" @ 1612325106789 = 3.0 ["code" => "200", "quantile" => "0.99"],
                    "_sum" @ 1612325106789 = 12.0 ["code" => "200"],
                    "_count" @ 1612325106789 = 6.0 ["code" => "200"]
                ]
            )
        );
    }

    fn encode_summary<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "requests".to_owned(),
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vector_lib::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0],
                count: 6,
                sum: 12.0,
            },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("ns"), &[], &[], &metric)
    }

    #[test]
    fn encodes_distribution_summary_text() {
        assert_eq!(
            encode_distribution_summary::<StringCollector>(),
            indoc! {r#"
                # HELP ns_requests requests
                # TYPE ns_requests summary
                ns_requests{code="200",quantile="0.5"} 2 1612325106789
                ns_requests{code="200",quantile="0.75"} 2 1612325106789
                ns_requests{code="200",quantile="0.9"} 3 1612325106789
                ns_requests{code="200",quantile="0.95"} 3 1612325106789
                ns_requests{code="200",quantile="0.99"} 3 1612325106789
                ns_requests_sum{code="200"} 15 1612325106789
                ns_requests_count{code="200"} 8 1612325106789
                ns_requests_min{code="200"} 1 1612325106789
                ns_requests_max{code="200"} 3 1612325106789
                ns_requests_avg{code="200"} 1.875 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_distribution_summary_request() {
        assert_eq!(
            encode_distribution_summary::<TimeSeries>(),
            write_request!(
                "ns_requests", "requests", Summary [
                    "" @ 1612325106789 = 2.0 ["code" => "200", "quantile" => "0.5"],
                    "" @ 1612325106789 = 2.0 ["code" => "200", "quantile" => "0.75"],
                    "" @ 1612325106789 = 3.0 ["code" => "200", "quantile" => "0.9"],
                    "" @ 1612325106789 = 3.0 ["code" => "200", "quantile" => "0.95"],
                    "" @ 1612325106789 = 3.0 ["code" => "200", "quantile" => "0.99"],
                    "_sum" @ 1612325106789 = 15.0 ["code" => "200"],
                    "_count" @ 1612325106789 = 8.0 ["code" => "200"],
                    "_min" @ 1612325106789 = 1.0 ["code" => "200"],
                    "_max" @ 1612325106789 = 3.0 ["code" => "200"],
                    "_avg" @ 1612325106789 = 1.875 ["code" => "200"]
                ]
            )
        );
    }

    fn encode_distribution_summary<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "requests".to_owned(),
            MetricKind::Absolute,
            MetricValue::Distribution {
                samples: vector_lib::samples![1.0 => 3, 2.0 => 3, 3.0 => 2],
                statistic: StatisticKind::Summary,
            },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(Some("ns"), &[], &default_summary_quantiles(), &metric)
    }

    #[test]
    fn encodes_timestamp_text() {
        assert_eq!(
            encode_timestamp::<StringCollector>(),
            indoc! {r"
                # HELP temperature temperature
                # TYPE temperature counter
                temperature 2 1612325106789
            "}
        );
    }

    #[test]
    fn encodes_timestamp_request() {
        assert_eq!(
            encode_timestamp::<TimeSeries>(),
            write_request!("temperature", "temperature", Counter ["" @ 1612325106789 = 2.0 []])
        );
    }

    fn encode_timestamp<T: MetricCollector>() -> T::Output {
        let metric = Metric::new(
            "temperature".to_owned(),
            MetricKind::Absolute,
            MetricValue::Counter { value: 2.0 },
        )
        .with_timestamp(Some(timestamp()));
        encode_one::<T>(None, &[], &[], &metric)
    }

    #[test]
    fn adds_timestamp_request() {
        let now = Utc::now().timestamp_millis();
        let metric = Metric::new(
            "something".to_owned(),
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.0 },
        );
        let encoded = encode_one::<TimeSeries>(None, &[], &[], &metric);
        assert!(encoded.timeseries[0].samples[0].timestamp >= now);
    }

    fn timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2021, 2, 3, 4, 5, 6)
            .single()
            .and_then(|t| t.with_nanosecond(789 * 1_000_000))
            .expect("invalid timestamp")
    }

    #[test]
    fn escapes_tags_text() {
        let tags = metric_tags!(
            "code" => "200",
            "quoted" => r#"host"1""#,
            "path" => r"c:\Windows",
        );
        let metric = Metric::new(
            "something".to_owned(),
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags));
        let encoded = encode_one::<StringCollector>(None, &[], &[], &metric);
        assert_eq!(
            encoded,
            indoc! {r#"
                # HELP something something
                # TYPE something counter
                something{code="200",path="c:\\Windows",quoted="host\"1\""} 1
            "#}
        );
    }

    /// According to the [spec](https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md?plain=1#L115)
    ///
    /// > Label names MUST be unique within a LabelSet.
    ///
    /// Prometheus itself will reject the metric with an error. Largely to remain backward
    /// compatible with older versions of Vector, we only publish the last tag in the list.
    #[test]
    fn encodes_duplicate_tags() {
        let tags = metric_tags!(
            "code" => "200",
            "code" => "success",
        );
        let metric = Metric::new(
            "something".to_owned(),
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(tags));
        let encoded = encode_one::<StringCollector>(None, &[], &[], &metric);
        assert_eq!(
            encoded,
            indoc! {r#"
                # HELP something something
                # TYPE something counter
                something{code="success"} 1
            "#}
        );
    }

    fn native_histogram_metric() -> Metric {
        use crate::event::metric::{
            NativeHistogramBuckets, NativeHistogramCount, NativeHistogramResetHint,
            NativeHistogramSpan,
        };

        // A simple native histogram with schema=0 (powers of 2), 3 populated positive buckets
        // starting at index 1: buckets at indices 1, 2, 3 with counts 2, 3, 1.
        // Delta encoding: [2, 1, -2] -> absolute counts [2, 3, 1]
        // Upper bounds at schema=0: index 1 -> 2.0, index 2 -> 4.0, index 3 -> 8.0
        Metric::new(
            "request_latency".to_owned(),
            MetricKind::Absolute,
            MetricValue::NativeHistogram {
                count: NativeHistogramCount::Integer(6),
                sum: 18.5,
                schema: 0,
                zero_threshold: 0.0,
                zero_count: NativeHistogramCount::Integer(0),
                positive_spans: vec![NativeHistogramSpan {
                    offset: 1,
                    length: 3,
                }],
                positive_buckets: NativeHistogramBuckets::IntegerDeltas(vec![2, 1, -2]),
                negative_spans: vec![],
                negative_buckets: NativeHistogramBuckets::IntegerDeltas(vec![]),
                reset_hint: NativeHistogramResetHint::No,
            },
        )
        .with_tags(Some(tags()))
        .with_timestamp(Some(timestamp()))
    }

    #[test]
    fn encodes_native_histogram_as_text_fallback() {
        // Text exposition format doesn't support native histograms, so we expect a lossy
        // conversion to classic bucketed histogram.
        let encoded = encode_one::<StringCollector>(None, &[], &[], &native_histogram_metric());
        assert_eq!(
            encoded,
            indoc! {r#"
                # HELP request_latency request_latency
                # TYPE request_latency histogram
                request_latency_bucket{code="200",le="2"} 2 1612325106789
                request_latency_bucket{code="200",le="4"} 5 1612325106789
                request_latency_bucket{code="200",le="8"} 6 1612325106789
                request_latency_bucket{code="200",le="+Inf"} 6 1612325106789
                request_latency_sum{code="200"} 18.5 1612325106789
                request_latency_count{code="200"} 6 1612325106789
            "#}
        );
    }

    #[test]
    fn encodes_native_histogram_as_remote_write() {
        // Remote write supports native histograms directly - verify we emit a proper
        // proto::Histogram rather than expanding to multiple samples.
        let encoded = encode_one::<TimeSeries>(None, &[], &[], &native_histogram_metric());

        assert_eq!(encoded.timeseries.len(), 1);
        let ts = &encoded.timeseries[0];
        assert!(ts.samples.is_empty(), "expected no float samples");
        assert_eq!(ts.histograms.len(), 1);

        let h = &ts.histograms[0];
        assert_eq!(h.schema, 0);
        assert_eq!(h.sum, 18.5);
        assert_eq!(h.count, Some(proto::histogram::Count::CountInt(6)));
        assert_eq!(h.positive_spans.len(), 1);
        assert_eq!(h.positive_spans[0].offset, 1);
        assert_eq!(h.positive_spans[0].length, 3);
        assert_eq!(h.positive_deltas, vec![2, 1, -2]);
        assert!(h.positive_counts.is_empty());
        assert_eq!(h.reset_hint, proto::histogram::ResetHint::No as i32);
        assert_eq!(h.timestamp, 1612325106789);

        // Verify labels include __name__ without suffix.
        assert!(
            ts.labels
                .iter()
                .any(|l| { l.name == "__name__" && l.value == "request_latency" })
        );
    }
}

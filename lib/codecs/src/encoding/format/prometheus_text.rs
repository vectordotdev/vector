use std::{collections::BTreeMap, fmt::Write as _};

use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{Event, Metric, MetricKind, MetricValue, metric::MetricTags},
    schema,
};

/// Config used to build a `PrometheusTextSerializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PrometheusTextSerializerConfig;

impl PrometheusTextSerializerConfig {
    /// Creates a new `PrometheusTextSerializerConfig`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Build the `PrometheusTextSerializer` from this configuration.
    #[must_use]
    pub const fn build(&self) -> PrometheusTextSerializer {
        PrometheusTextSerializer
    }

    /// The data type of events that are accepted by `PrometheusTextSerializer`.
    #[must_use]
    pub fn input_type(&self) -> DataType {
        DataType::Metric
    }

    /// The schema required by the serializer.
    #[must_use]
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts a metric to bytes using the [Prometheus text exposition
/// format][format].
///
/// Every event is encoded on its own: the output is a `# TYPE` line followed by one line per
/// series, and every line ends with a line feed. [`Self::encode_batch`] encodes several events as
/// one exposition instead.
///
/// Only absolute counters, gauges, sets, aggregated histograms and aggregated summaries can be
/// encoded. Other metrics are rejected, because turning them into series needs state or
/// settings (such as histogram buckets) that a codec does not have.
///
/// [format]: https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format
#[derive(Debug, Clone)]
pub struct PrometheusTextSerializer;

impl PrometheusTextSerializer {
    /// Encodes the events as one exposition, in which every metric family has a single `# TYPE`
    /// line followed by all of its series. Families are sorted by name.
    ///
    /// # Errors
    ///
    /// Returns an error, and writes nothing, if an event cannot be encoded or if two metrics of
    /// one family have different types.
    pub fn encode_batch(
        &self,
        events: impl IntoIterator<Item = Event>,
        buffer: &mut BytesMut,
    ) -> Result<(), vector_common::Error> {
        let mut families = BTreeMap::<String, (&str, String)>::new();
        for event in events {
            let Event::Metric(metric) = event else {
                return Err("the `prometheus_text` codec can only encode metrics".into());
            };
            let (name, kind, series) = encode_metric(&metric)?;
            match families.get_mut(&name) {
                Some((family_kind, _)) if *family_kind != kind => {
                    return Err(
                        format!("metric `{name}` is both a {family_kind} and a {kind}").into(),
                    );
                }
                Some((_, family_series)) => family_series.push_str(&series),
                None => {
                    families.insert(name, (kind, series));
                }
            }
        }

        for (name, (kind, series)) in families {
            buffer.extend_from_slice(format!("# TYPE {name} {kind}\n{series}").as_bytes());
        }
        Ok(())
    }
}

impl Encoder<Event> for PrometheusTextSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        self.encode_batch([event], buffer)
    }
}

/// Returns the family name, the type and the series lines of the metric.
#[allow(
    clippy::cast_precision_loss,
    reason = "Prometheus values are 64-bit floats."
)]
fn encode_metric(metric: &Metric) -> Result<(String, &'static str, String), vector_common::Error> {
    if metric.kind() != MetricKind::Absolute {
        return Err(format!(
            "the `prometheus_text` codec can only encode absolute metrics, but `{}` is \
             incremental; use the `incremental_to_absolute` transform before the sink",
            metric.name()
        )
        .into());
    }

    let name = match metric.namespace() {
        Some(namespace) => format!("{namespace}_{}", metric.name()),
        None => metric.name().to_string(),
    };
    validate_names(&name, metric)?;

    let tags = metric.tags();
    let timestamp = metric.timestamp().map(|t| t.timestamp_millis());
    let mut out = String::new();
    let mut emit = |suffix: &str, value: f64, extra: Option<(&str, String)>| {
        out.push_str(&name);
        out.push_str(suffix);
        encode_tags(&mut out, tags, extra);
        _ = match timestamp {
            None => writeln!(out, " {value}"),
            Some(timestamp) => writeln!(out, " {value} {timestamp}"),
        };
    };

    let kind = match metric.value() {
        MetricValue::Counter { value } => {
            emit("", *value, None);
            "counter"
        }
        MetricValue::Gauge { value } => {
            emit("", *value, None);
            "gauge"
        }
        MetricValue::Set { values } => {
            emit("", values.len() as f64, None);
            "gauge"
        }
        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            // Prometheus buckets are cumulative, and the `+Inf` bucket is always the total count,
            // so a bucket with an infinite upper limit is not encoded twice.
            let mut bucket_count = 0.0;
            for bucket in buckets.iter().filter(|b| !b.upper_limit.is_infinite()) {
                bucket_count += bucket.count as f64;
                emit(
                    "_bucket",
                    bucket_count,
                    Some(("le", bucket.upper_limit.to_string())),
                );
            }
            emit("_bucket", *count as f64, Some(("le", "+Inf".to_string())));
            // The sum is optional, and a made up zero would be wrong.
            if let Some(sum) = sum {
                emit("_sum", *sum, None);
            }
            emit("_count", *count as f64, None);
            "histogram"
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            for quantile in quantiles {
                emit(
                    "",
                    quantile.value,
                    Some(("quantile", quantile.quantile.to_string())),
                );
            }
            emit("_sum", *sum, None);
            emit("_count", *count as f64, None);
            "summary"
        }
        MetricValue::Distribution { .. } | MetricValue::Sketch { .. } => {
            return Err(format!(
                "the `prometheus_text` codec cannot encode `{}` as a {}; aggregate it into a \
                 histogram or a summary first",
                metric.name(),
                metric.value().as_name()
            )
            .into());
        }
    };

    Ok((name, kind, out))
}

/// Rejects a metric name or tag name that is not a valid Prometheus name, and a tag that would
/// duplicate a label the codec adds.
fn validate_names(name: &str, metric: &Metric) -> Result<(), vector_common::Error> {
    if !is_valid_name(name, true) {
        return Err(format!("`{name}` is not a valid Prometheus metric name").into());
    }
    let tags = metric.tags();
    if let Some(key) = tags
        .into_iter()
        .flat_map(MetricTags::iter_single)
        .map(|(key, _)| key)
        .find(|key| !is_valid_name(key, false))
    {
        return Err(format!(
            "metric `{name}` has the tag `{key}`, which is not a valid Prometheus label name"
        )
        .into());
    }
    let reserved = match metric.value() {
        MetricValue::AggregatedHistogram { .. } => Some("le"),
        MetricValue::AggregatedSummary { .. } => Some("quantile"),
        _ => None,
    };
    if let Some(label) = reserved
        && tags.is_some_and(|tags| tags.iter_single().any(|(key, _)| key == label))
    {
        return Err(format!("metric `{name}` has the reserved tag `{label}`").into());
    }
    Ok(())
}

/// Matches `[a-zA-Z_:][a-zA-Z0-9_:]*`, without the colons for a label name.
fn is_valid_name(name: &str, allow_colon: bool) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || byte == b'_'
                || (allow_colon && byte == b':')
                || (index > 0 && byte.is_ascii_digit())
        })
}

fn encode_tags(out: &mut String, tags: Option<&MetricTags>, extra: Option<(&str, String)>) {
    let mut parts = tags
        .into_iter()
        .flat_map(MetricTags::iter_single)
        .map(|(key, value)| format_tag(key, value))
        .collect::<Vec<_>>();
    if let Some((key, value)) = extra {
        parts.push(format_tag(key, &value));
    }
    if !parts.is_empty() {
        parts.sort();
        _ = write!(out, "{{{}}}", parts.join(","));
    }
}

fn format_tag(key: &str, value: &str) -> String {
    let mut result = String::with_capacity(key.len() + value.len() + 3);
    result.push_str(key);
    result.push_str("=\"");
    for character in value.chars() {
        match character {
            '\\' => result.push_str(r"\\"),
            '"' => result.push_str(r#"\""#),
            '\n' => result.push_str(r"\n"),
            character => result.push(character),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use vector_core::{
        event::{
            LogEvent,
            metric::{Bucket, Quantile, StatisticKind},
        },
        metric_tags,
    };

    use super::*;

    fn encode(metric: Metric) -> Result<String, vector_common::Error> {
        let mut buffer = BytesMut::new();
        PrometheusTextSerializerConfig
            .build()
            .encode(Event::Metric(metric), &mut buffer)?;
        Ok(String::from_utf8(buffer.to_vec()).unwrap())
    }

    fn absolute(name: &str, value: MetricValue) -> Metric {
        Metric::new(name, MetricKind::Absolute, value)
    }

    #[test]
    fn encodes_a_gauge_without_tags() {
        let text = encode(absolute("up", MetricValue::Gauge { value: 1.0 })).unwrap();
        assert_eq!(text, "# TYPE up gauge\nup 1\n");
    }

    #[test]
    fn encodes_a_counter_with_namespace_tags_and_timestamp() {
        let metric = absolute("requests_total", MetricValue::Counter { value: 42.0 })
            .with_namespace(Some("app"))
            .with_tags(Some(metric_tags!("method" => "GET", "code" => "200")))
            .with_timestamp(Some(
                Utc.with_ymd_and_hms(2026, 11, 23, 22, 16, 0)
                    .unwrap()
                    .with_timezone(&Utc),
            ));
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE app_requests_total counter\n\
             app_requests_total{code=\"200\",method=\"GET\"} 42 1795472160000\n"
        );
    }

    #[test]
    fn encodes_a_set_as_its_cardinality() {
        let metric = absolute(
            "users",
            MetricValue::Set {
                values: ["a", "b", "c"].into_iter().map(String::from).collect(),
            },
        );
        assert_eq!(encode(metric).unwrap(), "# TYPE users gauge\nusers 3\n");
    }

    #[test]
    fn escapes_tag_values() {
        let metric = absolute("m", MetricValue::Gauge { value: 1.0 })
            .with_tags(Some(metric_tags!("path" => "a\\b\"c\nd")));
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE m gauge\nm{path=\"a\\\\b\\\"c\\nd\"} 1\n"
        );
    }

    #[test]
    fn encodes_an_aggregated_histogram() {
        let metric = absolute(
            "latency",
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 0.5,
                        count: 2,
                    },
                    Bucket {
                        upper_limit: 1.0,
                        count: 3,
                    },
                    Bucket {
                        upper_limit: f64::INFINITY,
                        count: 1,
                    },
                ],
                count: 6,
                sum: Some(4.5),
            },
        );
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE latency histogram\n\
             latency_bucket{le=\"0.5\"} 2\n\
             latency_bucket{le=\"1\"} 5\n\
             latency_bucket{le=\"+Inf\"} 6\n\
             latency_sum 4.5\n\
             latency_count 6\n"
        );
    }

    #[test]
    fn sorts_the_bucket_label_among_the_tags() {
        let metric = absolute(
            "latency",
            MetricValue::AggregatedHistogram {
                buckets: vec![Bucket {
                    upper_limit: 1.0,
                    count: 1,
                }],
                count: 1,
                sum: None,
            },
        )
        .with_tags(Some(metric_tags!("job" => "api", "zone" => "a")));
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE latency histogram\n\
             latency_bucket{job=\"api\",le=\"1\",zone=\"a\"} 1\n\
             latency_bucket{job=\"api\",le=\"+Inf\",zone=\"a\"} 1\n\
             latency_count{job=\"api\",zone=\"a\"} 1\n"
        );
    }

    #[test]
    fn leaves_out_a_missing_histogram_sum() {
        let metric = absolute(
            "latency",
            MetricValue::AggregatedHistogram {
                buckets: vec![],
                count: 0,
                sum: None,
            },
        );
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE latency histogram\nlatency_bucket{le=\"+Inf\"} 0\nlatency_count 0\n"
        );
    }

    #[test]
    fn encodes_an_aggregated_summary() {
        let metric = absolute(
            "duration",
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.5,
                        value: 1.5,
                    },
                    Quantile {
                        quantile: 0.99,
                        value: 9.0,
                    },
                ],
                count: 10,
                sum: 20.0,
            },
        );
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE duration summary\n\
             duration{quantile=\"0.5\"} 1.5\n\
             duration{quantile=\"0.99\"} 9\n\
             duration_sum 20\n\
             duration_count 10\n"
        );
    }

    #[test]
    fn rejects_an_incremental_metric() {
        let metric = Metric::new(
            "hits",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );
        let error = encode(metric).unwrap_err().to_string();
        assert!(error.contains("incremental_to_absolute"), "{error}");
    }

    #[test]
    fn rejects_a_distribution() {
        let metric = absolute(
            "latency",
            MetricValue::Distribution {
                samples: vec![],
                statistic: StatisticKind::Histogram,
            },
        );
        assert!(encode(metric).is_err());
    }

    #[test]
    fn rejects_line_breaks_in_names() {
        let metric = absolute("a\nb", MetricValue::Gauge { value: 1.0 });
        assert!(encode(metric).is_err());
    }

    #[test]
    fn encodes_a_batch_with_one_type_line_per_family() {
        let gauge = |host: &str| {
            absolute("up", MetricValue::Gauge { value: 1.0 })
                .with_tags(Some(metric_tags!("host" => host)))
        };
        let histogram = |host: &str| {
            absolute(
                "latency",
                MetricValue::AggregatedHistogram {
                    buckets: vec![],
                    count: 1,
                    sum: None,
                },
            )
            .with_tags(Some(metric_tags!("host" => host)))
        };
        let events = [gauge("a"), histogram("a"), gauge("b"), histogram("b")].map(Event::Metric);

        let mut buffer = BytesMut::new();
        PrometheusTextSerializerConfig
            .build()
            .encode_batch(events, &mut buffer)
            .unwrap();
        assert_eq!(
            String::from_utf8(buffer.to_vec()).unwrap(),
            "# TYPE latency histogram\n\
             latency_bucket{host=\"a\",le=\"+Inf\"} 1\n\
             latency_count{host=\"a\"} 1\n\
             latency_bucket{host=\"b\",le=\"+Inf\"} 1\n\
             latency_count{host=\"b\"} 1\n\
             # TYPE up gauge\n\
             up{host=\"a\"} 1\n\
             up{host=\"b\"} 1\n"
        );
    }

    #[test]
    fn rejects_a_batch_with_two_types_for_one_family() {
        let events = [
            absolute("up", MetricValue::Gauge { value: 1.0 }),
            absolute("up", MetricValue::Counter { value: 1.0 }),
        ]
        .map(Event::Metric);

        let mut buffer = BytesMut::new();
        let error = PrometheusTextSerializerConfig
            .build()
            .encode_batch(events, &mut buffer)
            .unwrap_err()
            .to_string();
        assert!(error.contains("both a gauge and a counter"), "{error}");
        assert!(buffer.is_empty());
    }

    #[test]
    fn rejects_names_that_are_not_prometheus_identifiers() {
        let gauge = || absolute("up", MetricValue::Gauge { value: 1.0 });
        for metric in [
            absolute("foo.bar", MetricValue::Gauge { value: 1.0 }),
            absolute("1up", MetricValue::Gauge { value: 1.0 }),
            gauge().with_namespace(Some("my.app")),
            gauge().with_tags(Some(metric_tags!("http.method" => "GET"))),
            gauge().with_tags(Some(metric_tags!("0zone" => "a"))),
            gauge().with_tags(Some(metric_tags!("a:b" => "c"))),
        ] {
            assert!(encode(metric.clone()).is_err(), "{metric:?}");
        }
    }

    #[test]
    fn accepts_colons_in_metric_names() {
        let metric = absolute("job:requests:rate5m", MetricValue::Gauge { value: 1.0 });
        assert_eq!(
            encode(metric).unwrap(),
            "# TYPE job:requests:rate5m gauge\njob:requests:rate5m 1\n"
        );
    }

    #[test]
    fn rejects_tags_named_like_generated_labels() {
        let histogram = absolute(
            "latency",
            MetricValue::AggregatedHistogram {
                buckets: vec![],
                count: 0,
                sum: None,
            },
        )
        .with_tags(Some(metric_tags!("le" => "tenant")));
        let summary = absolute(
            "duration",
            MetricValue::AggregatedSummary {
                quantiles: vec![],
                count: 0,
                sum: 0.0,
            },
        )
        .with_tags(Some(metric_tags!("quantile" => "x")));
        for metric in [histogram, summary] {
            let error = encode(metric).unwrap_err().to_string();
            assert!(error.contains("reserved"), "{error}");
        }
    }

    #[test]
    fn rejects_a_log() {
        let mut buffer = BytesMut::new();
        let result = PrometheusTextSerializerConfig
            .build()
            .encode(Event::Log(LogEvent::from_str_legacy("foo")), &mut buffer);
        assert!(result.is_err());
    }
}

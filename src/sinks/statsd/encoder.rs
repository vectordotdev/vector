use std::fmt::Display;

use bytes::{BufMut, BytesMut};
use tokio_util::codec::Encoder;
use vector_core::event::{Metric, MetricKind, MetricTags, MetricValue, StatisticKind};

use crate::{
    internal_events::StatsdInvalidMetricError,
    sinks::util::{buffer::metrics::compress_distribution, encode_namespace},
};

#[derive(Debug, Clone)]
pub struct StatsdEncoder {
    default_namespace: Option<String>,
}

impl StatsdEncoder {
    /// Creates a new `StatsdEncoder` with the given default namespace, if any.
    pub const fn new(default_namespace: Option<String>) -> Self {
        Self { default_namespace }
    }
}

impl<'a> Encoder<&'a Metric> for StatsdEncoder {
    type Error = codecs::encoding::Error;

    fn encode(&mut self, metric: &'a Metric, bytes: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buf = Vec::new();

        match metric.value() {
            MetricValue::Counter { value } => {
                push_event(&mut buf, metric, value, "c", None);
            }
            MetricValue::Gauge { value } => {
                match metric.kind() {
                    MetricKind::Incremental => {
                        push_event(&mut buf, metric, format!("{:+}", value), "g", None)
                    }
                    MetricKind::Absolute => push_event(&mut buf, metric, value, "g", None),
                };
            }
            MetricValue::Distribution { samples, statistic } => {
                let metric_type = match statistic {
                    StatisticKind::Histogram => "h",
                    StatisticKind::Summary => "d",
                };

                // TODO: This would actually be good to potentially add a helper combinator for, in the same vein as
                // `SinkBuilderExt::normalized`, that provides a metric "optimizer" for doing these sorts of things. We
                // don't actually compress distributions as-is in other metrics sinks unless they use the old-style
                // approach coupled with `MetricBuffer`. While not every sink would benefit from this -- the
                // `datadog_metrics` sink always converts distributions to sketches anyways, for example -- a lot of
                // them could.
                //
                // This would also imply rewriting this sink in the new style to take advantage of it.
                let mut samples = samples.clone();
                let compressed_samples = compress_distribution(&mut samples);
                for sample in compressed_samples {
                    push_event(
                        &mut buf,
                        metric,
                        sample.value,
                        metric_type,
                        Some(sample.rate),
                    );
                }
            }
            MetricValue::Set { values } => {
                for val in values {
                    push_event(&mut buf, metric, val, "s", None);
                }
            }
            _ => {
                emit!(StatsdInvalidMetricError {
                    value: metric.value(),
                    kind: metric.kind(),
                });

                return Ok(());
            }
        };

        let message = encode_namespace(
            metric.namespace().or(self.default_namespace.as_deref()),
            '.',
            buf.join("|"),
        );

        bytes.put_slice(&message.into_bytes());
        bytes.put_u8(b'\n');

        Ok(())
    }
}

// Note that if multi-valued tags are present, this encoding may change the order from the input
// event, since the tags with multiple values may not have been grouped together.
// This is not an issue, but noting as it may be an observed behavior.
fn encode_tags(tags: &MetricTags) -> String {
    let parts: Vec<_> = tags
        .iter_all()
        .map(|(name, tag_value)| match tag_value {
            Some(value) => format!("{}:{}", name, value),
            None => name.to_owned(),
        })
        .collect();

    // `parts` is already sorted by key because of BTreeMap
    parts.join(",")
}

fn push_event<V: Display>(
    buf: &mut Vec<String>,
    metric: &Metric,
    val: V,
    metric_type: &str,
    sample_rate: Option<u32>,
) {
    buf.push(format!("{}:{}|{}", metric.name(), val, metric_type));

    if let Some(sample_rate) = sample_rate {
        if sample_rate != 1 {
            buf.push(format!("@{}", 1.0 / f64::from(sample_rate)))
        }
    };

    if let Some(t) = metric.tags() {
        buf.push(format!("#{}", encode_tags(t)));
    };
}
#[cfg(test)]
mod tests {
    use std::str::from_utf8;

    use bytes::BytesMut;
    use tokio_util::codec::Encoder;
    use vector_core::{
        event::{metric::TagValue, MetricKind, MetricTags, MetricValue, StatisticKind},
        metric_tags,
    };

    use super::{encode_tags, StatsdEncoder};
    use crate::event::Metric;

    #[cfg(feature = "sources-statsd")]
    use crate::sources::statsd::parser::parse;

    fn tags() -> MetricTags {
        metric_tags!(
            "normal_tag" => "value",
            "multi_value" => "true",
            "multi_value" => "false",
            "multi_value" => TagValue::Bare,
            "bare_tag" => TagValue::Bare,
        )
    }

    #[test]
    fn test_encode_tags() {
        let actual = encode_tags(&tags());
        let mut actual = actual.split(',').collect::<Vec<_>>();
        actual.sort();

        let mut expected =
            "bare_tag,normal_tag:value,multi_value:true,multi_value:false,multi_value"
                .split(',')
                .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn tags_order() {
        assert_eq!(
            &encode_tags(
                &vec![
                    ("a", "value"),
                    ("b", "value"),
                    ("c", "value"),
                    ("d", "value"),
                    ("e", "value"),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect()
            ),
            "a:value,b:value,c:value,d:value,e:value"
        );
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_counter() {
        let metric1 = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.5 },
        )
        .with_tags(Some(tags()));
        let event = metric1.clone();
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_counter() {
        let event = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.5 },
        );
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        // The statsd parser will parse the counter as Incremental,
        // so we can't compare it with the parsed value.
        assert_eq!("counter:1.5|c\n", from_utf8(&frame).unwrap());
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_gauge() {
        let metric1 = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -1.5 },
        )
        .with_tags(Some(tags()));
        let event = metric1.clone();
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_gauge() {
        let metric1 = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.5 },
        )
        .with_tags(Some(tags()));
        let event = metric1.clone();
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_distribution() {
        let metric1 = Metric::new(
            "distribution",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![1.5 => 1, 1.5 => 1],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_tags(Some(tags()));

        let metric1_compressed = Metric::new(
            "distribution",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![1.5 => 2],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_tags(Some(tags()));

        let event = metric1;
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1_compressed, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_set() {
        let metric1 = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["abc".to_owned()].into_iter().collect(),
            },
        )
        .with_tags(Some(tags()));
        let event = metric1.clone();
        let mut encoder = StatsdEncoder {
            default_namespace: None,
        };
        let mut frame = BytesMut::new();
        encoder.encode(&event, &mut frame).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        vector_common::assert_event_data_eq!(metric1, metric2);
    }
}

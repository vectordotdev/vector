use std::fmt::Display;

use bytes::BytesMut;
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
        Self {
            default_namespace,
        }
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
                    kind: &metric.kind(),
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

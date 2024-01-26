use std::collections::VecDeque;

use vector_lib::event::{metric::MetricData, Metric, MetricValue};

#[allow(clippy::large_enum_variant)]
enum SplitState {
    Single(Option<Metric>),
    Multiple(VecDeque<Metric>),
}

/// An iterator that returns the result of a metric split operation.
pub struct SplitIterator {
    state: SplitState,
}

impl SplitIterator {
    /// Creates an iterator for a single metric.
    pub const fn single(metric: Metric) -> Self {
        Self {
            state: SplitState::Single(Some(metric)),
        }
    }

    /// Creates an iterator for multiple metrics.
    pub fn multiple<I>(metrics: I) -> Self
    where
        I: Into<VecDeque<Metric>>,
    {
        Self {
            state: SplitState::Multiple(metrics.into()),
        }
    }
}

impl Iterator for SplitIterator {
    type Item = Metric;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            SplitState::Single(metric) => metric.take(),
            SplitState::Multiple(metrics) => metrics.pop_front(),
        }
    }
}

/// Splits a metric into potentially multiple metrics.
///
/// In some cases, a single metric may represent multiple fundamental metrics: an aggregated summary or histogram can
/// represent a count, sum, and subtotals for a given measurement. These metrics may be able to be handled
/// natively/directly in a sink, but in other cases, those fundamental metrics may need to be extracted and operated on individually.
///
/// This trait defines a simple interface for defining custom rules about what metrics to split and when to split them.
pub trait MetricSplit {
    /// Attempts to split the metric.
    ///
    /// The returned iterator will either return only the input metric if no splitting occurred, or all resulting
    /// metrics that were created as a result of the split.
    fn split(&mut self, input: Metric) -> SplitIterator;
}

/// A self-contained metric splitter.
///
/// The splitter state is stored internally, and it can only be created from a splitter implementation that is either
/// `Default` or is constructed ahead of time, so it is primarily useful for constructing a usable splitter via implicit
/// conversion methods or when no special parameters are required for configuring the underlying splitter.
pub struct MetricSplitter<S> {
    splitter: S,
}

impl<S: MetricSplit> MetricSplitter<S> {
    /// Attempts to split the metric.
    ///
    /// For more information about splitting, see the documentation for [`MetricSplit::split`].
    pub fn split(&mut self, input: Metric) -> SplitIterator {
        self.splitter.split(input)
    }
}

impl<S: Default> Default for MetricSplitter<S> {
    fn default() -> Self {
        Self {
            splitter: S::default(),
        }
    }
}

impl<S> From<S> for MetricSplitter<S> {
    fn from(splitter: S) -> Self {
        Self { splitter }
    }
}

/// A splitter that separates an aggregated summary into its various parts.
///
/// Generally speaking, all metric types supported by Vector have way to be added to and removed from other instances of
/// themselves, such as merging two counters by adding together their values, or merging two distributions simply be
/// adding all of their samples together.
///
/// However, one particular metric type is not amenable to these operations: aggregated summaries. Hailing from
/// Prometheus, aggregated summaries are meant to be client-side generated versions of summary data about a histogram:
/// count, sum, and various quantiles. As quantiles themselves cannot simply be added to or removed from each other
/// without entirely altering the statistical significancy of their value, we often do not do anything with them except
/// forwards them on directly as their individual pieces, or even drop them.
///
/// However, as many sinks must do this, this splitter exists to bundle the operation in a reusable piece of code that
/// all sinks needing to do so can share.
///
/// All other metric types are passed through as-is.
#[derive(Clone, Copy, Debug, Default)]
pub struct AggregatedSummarySplitter;

impl MetricSplit for AggregatedSummarySplitter {
    fn split(&mut self, input: Metric) -> SplitIterator {
        let (series, data, metadata) = input.into_parts();
        match data.value() {
            // If it's not an aggregated summary, just send it on semi-unchanged. :)
            MetricValue::Counter { .. }
            | MetricValue::Gauge { .. }
            | MetricValue::Set { .. }
            | MetricValue::Distribution { .. }
            | MetricValue::AggregatedHistogram { .. }
            | MetricValue::Sketch { .. } => {
                SplitIterator::single(Metric::from_parts(series, data, metadata))
            }
            MetricValue::AggregatedSummary { .. } => {
                // Further extract the aggregated summary components so we can generate our multiple metrics.
                let (time, kind, value) = data.into_parts();
                let (quantiles, count, sum) = match value {
                    MetricValue::AggregatedSummary {
                        quantiles,
                        count,
                        sum,
                    } => (quantiles, count, sum),
                    _ => unreachable!("metric value must be aggregated summary to be here"),
                };

                // We generate one metric for the count, one metric for the sum, and one metric for each quantile. We
                // clone the timestamp, kind, metadata, etc, to keep everything the same as it was on the way in.
                let mut metrics = VecDeque::new();

                let mut count_series = series.clone();
                count_series.name_mut().name_mut().push_str("_count");
                let count_data = MetricData::from_parts(
                    time,
                    kind,
                    MetricValue::Counter {
                        value: count as f64,
                    },
                );
                let count_metadata = metadata.clone();

                metrics.push_back(Metric::from_parts(count_series, count_data, count_metadata));

                for quantile in quantiles {
                    let mut quantile_series = series.clone();
                    quantile_series
                        .replace_tag(String::from("quantile"), quantile.to_quantile_string());
                    let quantile_data = MetricData::from_parts(
                        time,
                        kind,
                        MetricValue::Gauge {
                            value: quantile.value,
                        },
                    );
                    let quantile_metadata = metadata.clone();

                    metrics.push_back(Metric::from_parts(
                        quantile_series,
                        quantile_data,
                        quantile_metadata,
                    ));
                }

                let mut sum_series = series;
                sum_series.name_mut().name_mut().push_str("_sum");
                let sum_data =
                    MetricData::from_parts(time, kind, MetricValue::Counter { value: sum });
                let sum_metadata = metadata;

                metrics.push_back(Metric::from_parts(sum_series, sum_data, sum_metadata));

                SplitIterator::multiple(metrics)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use vector_lib::event::{
        metric::{Bucket, MetricTags, Quantile, Sample},
        Metric, MetricKind, MetricValue, StatisticKind,
    };

    use super::{AggregatedSummarySplitter, MetricSplitter};

    #[test]
    fn test_agg_summary_split() {
        let mut splitter: MetricSplitter<AggregatedSummarySplitter> = MetricSplitter::default();

        let counter = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 42.0 },
        );
        let gauge = Metric::new(
            "gauge",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 3.15 },
        );
        let set = Metric::new(
            "set",
            MetricKind::Absolute,
            MetricValue::Set {
                values: BTreeSet::from([String::from("foobar")]),
            },
        );
        let distribution = Metric::new(
            "distribution",
            MetricKind::Incremental,
            MetricValue::Distribution {
                statistic: StatisticKind::Histogram,
                samples: vec![Sample {
                    value: 13.37,
                    rate: 10,
                }],
            },
        );
        let agg_histo = Metric::new(
            "agg_histo",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 10.0,
                        count: 5,
                    },
                    Bucket {
                        upper_limit: 25.0,
                        count: 2,
                    },
                ],
                count: 7,
                sum: 100.0,
            },
        );
        let agg_summary = Metric::new(
            "agg_summary",
            MetricKind::Absolute,
            MetricValue::AggregatedSummary {
                quantiles: vec![
                    Quantile {
                        quantile: 0.05,
                        value: 10.0,
                    },
                    Quantile {
                        quantile: 0.95,
                        value: 25.0,
                    },
                ],
                count: 7,
                sum: 100.0,
            },
        );

        let quantile_tag = |q: f64| -> Option<MetricTags> {
            let quantile = Quantile {
                quantile: q,
                value: 0.0,
            };

            Some(
                vec![("quantile".to_owned(), quantile.to_quantile_string())]
                    .into_iter()
                    .collect(),
            )
        };

        let agg_summary_splits = vec![
            Metric::new(
                "agg_summary_count",
                MetricKind::Absolute,
                MetricValue::Counter { value: 7.0 },
            ),
            Metric::new(
                "agg_summary",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 10.0 },
            )
            .with_tags(quantile_tag(0.05)),
            Metric::new(
                "agg_summary",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 25.0 },
            )
            .with_tags(quantile_tag(0.95)),
            Metric::new(
                "agg_summary_sum",
                MetricKind::Absolute,
                MetricValue::Counter { value: 100.0 },
            ),
        ];

        let cases = &[
            (counter.clone(), vec![counter]),
            (gauge.clone(), vec![gauge]),
            (set.clone(), vec![set]),
            (distribution.clone(), vec![distribution]),
            (agg_histo.clone(), vec![agg_histo]),
            (agg_summary, agg_summary_splits),
        ];

        for (input, expected) in cases {
            let actual = splitter.split(input.clone()).collect::<Vec<_>>();
            assert_eq!(expected.clone(), actual);
        }
    }
}

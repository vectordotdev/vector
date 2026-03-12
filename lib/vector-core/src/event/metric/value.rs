use core::fmt;
use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use vector_common::byte_size_of::ByteSizeOf;
use vector_config::configurable_component;

use super::{samples_to_buckets, write_list, write_word};
use crate::{float_eq, metrics::AgentDDSketch};

const INFINITY: &str = "inf";
const NEG_INFINITY: &str = "-inf";
const NAN: &str = "NaN";

/// Metric value.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
/// Container for the actual value of a metric.
pub enum MetricValue {
    /// A cumulative numerical value that can only increase or be reset to zero.
    Counter {
        /// The value of the counter.
        value: f64,
    },

    /// A single numerical value that can arbitrarily go up and down.
    Gauge {
        /// The value of the gauge.
        value: f64,
    },

    /// A set of (unordered) unique values for a key.
    Set {
        /// The values in the set.
        values: BTreeSet<String>,
    },

    /// A set of observations without any aggregation or sampling.
    Distribution {
        /// The observed values within this distribution.
        samples: Vec<Sample>,

        /// The type of statistics to derive for this distribution.
        statistic: StatisticKind,
    },

    /// A set of observations which are counted into buckets.
    ///
    /// It also contains the total count of all observations and their sum to allow calculating the mean.
    AggregatedHistogram {
        /// The buckets within this histogram.
        buckets: Vec<Bucket>,

        /// The total number of observations contained within this histogram.
        count: u64,

        /// The sum of all observations contained within this histogram.
        sum: f64,
    },

    /// A set of observations which are represented by quantiles.
    ///
    /// Each quantile contains the upper value of the quantile (0 <= φ <= 1). It also contains the total count of all
    /// observations and their sum to allow calculating the mean.
    AggregatedSummary {
        /// The quantiles measured from this summary.
        quantiles: Vec<Quantile>,

        /// The total number of observations contained within this summary.
        count: u64,

        /// The sum of all observations contained within this histogram.
        sum: f64,
    },

    /// A data structure that can answer questions about the cumulative distribution of the contained samples in
    /// space-efficient way.
    ///
    /// Sketches represent the data in a way that queries over it have bounded error guarantees without needing to hold
    /// every single sample in memory. They are also, typically, able to be merged with other sketches of the same type
    /// such that client-side _and_ server-side aggregation can be accomplished without loss of accuracy in the queries.
    Sketch {
        #[configurable(derived)]
        sketch: MetricSketch,
    },

    /// A Prometheus-style native (exponential) histogram.
    ///
    /// Native histograms use exponential bucket boundaries determined by a `schema` parameter, allowing for high
    /// resolution at low cost. Unlike `AggregatedHistogram` which uses fixed bucket boundaries, native histograms use
    /// sparse buckets indexed by integer keys, where adjacent buckets grow by a factor of `2^(2^-schema)`.
    ///
    /// See <https://prometheus.io/docs/specs/native_histograms/> for details.
    NativeHistogram {
        /// The total number of observations.
        ///
        /// May be a float to support gauge histograms where resets can cause fractional counts.
        count: NativeHistogramCount,

        /// The sum of all observations.
        sum: f64,

        /// The resolution parameter.
        ///
        /// Valid values are from -4 to 8 for standard exponential schemas. Higher values give finer resolution.
        /// Bucket boundaries are at `(2^(2^-schema))^n` for positive buckets.
        schema: i32,

        /// The width of the "zero bucket".
        ///
        /// Observations in `[-zero_threshold, zero_threshold]` are counted in the zero bucket rather than in positive
        /// or negative exponential buckets.
        zero_threshold: f64,

        /// Count of observations in the zero bucket.
        zero_count: NativeHistogramCount,

        /// Spans of populated positive buckets.
        positive_spans: Vec<NativeHistogramSpan>,

        /// Bucket values for positive buckets.
        ///
        /// For integer counts, these are deltas from the previous bucket (first is absolute). For float counts, these
        /// are absolute values. The interpretation depends on the `count` type.
        positive_buckets: NativeHistogramBuckets,

        /// Spans of populated negative buckets.
        negative_spans: Vec<NativeHistogramSpan>,

        /// Bucket values for negative buckets.
        ///
        /// For integer counts, these are deltas from the previous bucket (first is absolute). For float counts, these
        /// are absolute values. The interpretation depends on the `count` type.
        negative_buckets: NativeHistogramBuckets,

        /// Hint about whether this represents a counter reset.
        reset_hint: NativeHistogramResetHint,
    },
}

impl MetricValue {
    /// Returns `true` if the value is empty.
    ///
    /// Emptiness is dictated by whether or not the value has any samples or measurements present. Consequently, scalar
    /// values (counter, gauge) are never considered empty.
    pub fn is_empty(&self) -> bool {
        match self {
            MetricValue::Counter { .. } | MetricValue::Gauge { .. } => false,
            MetricValue::Set { values } => values.is_empty(),
            MetricValue::Distribution { samples, .. } => samples.is_empty(),
            MetricValue::AggregatedSummary { count, .. }
            | MetricValue::AggregatedHistogram { count, .. } => *count == 0,
            MetricValue::Sketch { sketch } => sketch.is_empty(),
            MetricValue::NativeHistogram { count, .. } => count.as_f64() == 0.0,
        }
    }

    /// Gets the name of this value as a string.
    ///
    /// This maps to the name of the enum variant itself.
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::Counter { .. } => "counter",
            Self::Gauge { .. } => "gauge",
            Self::Set { .. } => "set",
            Self::Distribution { .. } => "distribution",
            Self::AggregatedHistogram { .. } => "aggregated histogram",
            Self::AggregatedSummary { .. } => "aggregated summary",
            Self::Sketch { sketch } => sketch.as_name(),
            Self::NativeHistogram { .. } => "native histogram",
        }
    }

    /// Converts a native histogram to an aggregated histogram.
    ///
    /// This is a **lossy** conversion: native histograms use exponential bucket boundaries determined by the schema,
    /// while aggregated histograms use fixed explicit boundaries. The resulting aggregated histogram will have one
    /// bucket per populated native histogram bucket, with upper limits computed from the schema.
    ///
    /// Negative buckets are merged together as observations below zero, and the zero bucket is placed at
    /// `zero_threshold`.
    ///
    /// If this value is not a native histogram, returns `None`.
    #[must_use]
    pub fn native_histogram_to_agg_histogram(&self) -> Option<MetricValue> {
        match self {
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
                ..
            } => {
                let mut buckets = Vec::new();

                // All negative observations collapse into one bucket at upper_limit = -zero_threshold (or 0 if
                // zero_threshold is 0). This is lossy but aggregated histograms don't naturally represent negative
                // exponential buckets.
                let neg_total: f64 = iter_span_counts(negative_spans, negative_buckets)
                    .map(|(_, c)| c)
                    .sum();
                if neg_total > 0.0 {
                    let limit = if *zero_threshold > 0.0 {
                        -*zero_threshold
                    } else {
                        0.0
                    };
                    buckets.push(Bucket {
                        upper_limit: limit,
                        // Truncation: fractional counts from float histograms are floored.
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        count: neg_total.max(0.0) as u64,
                    });
                }

                // Zero bucket.
                let zc = zero_count.as_f64();
                if zc > 0.0 {
                    buckets.push(Bucket {
                        upper_limit: *zero_threshold,
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        count: zc.max(0.0) as u64,
                    });
                }

                // Positive buckets: compute upper bound for each populated bucket.
                for (index, c) in iter_span_counts(positive_spans, positive_buckets) {
                    if c > 0.0 {
                        buckets.push(Bucket {
                            upper_limit: native_histogram_bucket_upper_bound(*schema, index),
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            count: c.max(0.0) as u64,
                        });
                    }
                }

                // Truncation: fractional counts from float histograms are floored.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let count_u64 = count.as_f64().max(0.0) as u64;

                Some(MetricValue::AggregatedHistogram {
                    buckets,
                    count: count_u64,
                    sum: *sum,
                })
            }
            _ => None,
        }
    }

    /// Converts a distribution to an aggregated histogram.
    ///
    /// Histogram bucket bounds are based on `buckets`, where the value is the upper bound of the bucket.  Samples will
    /// be thus be ordered in a "less than" fashion: if the given sample is less than or equal to a given bucket's upper
    /// bound, it will be counted towards that bucket at the given sample rate.
    ///
    /// If this value is not a distribution, then `None` is returned.  Otherwise,
    /// `Some(MetricValue::AggregatedHistogram)` is returned.
    pub fn distribution_to_agg_histogram(&self, buckets: &[f64]) -> Option<MetricValue> {
        match self {
            MetricValue::Distribution { samples, .. } => {
                let (buckets, count, sum) = samples_to_buckets(samples, buckets);

                Some(MetricValue::AggregatedHistogram {
                    buckets,
                    count,
                    sum,
                })
            }
            _ => None,
        }
    }

    /// Converts a distribution to a sketch.
    ///
    /// This conversion specifically use the `AgentDDSketch` sketch variant, in the default configuration that matches
    /// the Datadog Agent, parameter-wise.
    ///
    /// If this value is not a distribution, then `None` is returned.  Otherwise, `Some(MetricValue::Sketch)` is
    /// returned.
    pub fn distribution_to_sketch(&self) -> Option<MetricValue> {
        match self {
            MetricValue::Distribution { samples, .. } => {
                let mut sketch = AgentDDSketch::with_agent_defaults();
                for sample in samples {
                    sketch.insert_n(sample.value, sample.rate);
                }

                Some(MetricValue::Sketch {
                    sketch: MetricSketch::AgentDDSketch(sketch),
                })
            }
            _ => None,
        }
    }

    /// Zeroes out all the values contained in this value.
    ///
    /// This keeps all the bucket/value vectors for the histogram and summary metric types intact while zeroing the
    /// counts. Distribution metrics are emptied of all their values.
    pub fn zero(&mut self) {
        match self {
            Self::Counter { value } | Self::Gauge { value } => *value = 0.0,
            Self::Set { values } => values.clear(),
            Self::Distribution { samples, .. } => samples.clear(),
            Self::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                for bucket in buckets {
                    bucket.count = 0;
                }
                *count = 0;
                *sum = 0.0;
            }
            Self::AggregatedSummary {
                quantiles,
                sum,
                count,
            } => {
                for quantile in quantiles {
                    quantile.value = 0.0;
                }
                *count = 0;
                *sum = 0.0;
            }
            Self::Sketch { sketch } => match sketch {
                MetricSketch::AgentDDSketch(ddsketch) => {
                    ddsketch.clear();
                }
            },
            Self::NativeHistogram {
                count,
                sum,
                zero_count,
                positive_spans,
                positive_buckets,
                negative_spans,
                negative_buckets,
                ..
            } => {
                *count = NativeHistogramCount::default();
                *sum = 0.0;
                *zero_count = NativeHistogramCount::default();
                positive_spans.clear();
                *positive_buckets = NativeHistogramBuckets::default();
                negative_spans.clear();
                *negative_buckets = NativeHistogramBuckets::default();
            }
        }
    }

    /// Adds another value to this one.
    ///
    /// If the other value is not the same type, or if they are but their defining characteristics of the value are
    /// different (i.e. aggregated histograms with different bucket layouts), then `false` is returned.  Otherwise,
    /// `true` is returned.
    #[must_use]
    pub fn add(&mut self, other: &Self) -> bool {
        match (self, other) {
            (Self::Counter { value }, Self::Counter { value: value2 })
            | (Self::Gauge { value }, Self::Gauge { value: value2 }) => {
                *value += value2;
                true
            }
            (Self::Set { values }, Self::Set { values: values2 }) => {
                values.extend(values2.iter().map(Into::into));
                true
            }
            (
                Self::Distribution {
                    samples,
                    statistic: statistic_a,
                },
                Self::Distribution {
                    samples: samples2,
                    statistic: statistic_b,
                },
            ) if statistic_a == statistic_b => {
                samples.extend_from_slice(samples2);
                true
            }
            (
                Self::AggregatedHistogram {
                    buckets,
                    count,
                    sum,
                },
                Self::AggregatedHistogram {
                    buckets: buckets2,
                    count: count2,
                    sum: sum2,
                },
            ) if buckets.len() == buckets2.len()
                && buckets
                    .iter()
                    .zip(buckets2.iter())
                    .all(|(b1, b2)| b1.upper_limit == b2.upper_limit) =>
            {
                for (b1, b2) in buckets.iter_mut().zip(buckets2) {
                    b1.count += b2.count;
                }
                *count += count2;
                *sum += sum2;
                true
            }
            (Self::Sketch { sketch }, Self::Sketch { sketch: sketch2 }) => {
                match (sketch, sketch2) {
                    (
                        MetricSketch::AgentDDSketch(ddsketch),
                        MetricSketch::AgentDDSketch(ddsketch2),
                    ) => ddsketch.merge(ddsketch2).is_ok(),
                }
            }
            _ => false,
        }
    }

    /// Subtracts another value from this one.
    ///
    /// If the other value is not the same type, or if they are but their defining characteristics of the value are
    /// different (i.e. aggregated histograms with different bucket layouts), then `false` is returned.  Otherwise,
    /// `true` is returned.
    #[must_use]
    pub fn subtract(&mut self, other: &Self) -> bool {
        match (self, other) {
            // Counters are monotonic, they should _never_ go backwards unless reset to 0 due to
            // process restart, etc.  Thus, being able to generate negative deltas would violate
            // that.  Whether a counter is reset to 0, or if it incorrectly warps to a previous
            // value, it doesn't matter: we're going to reinitialize it.
            (Self::Counter { value }, Self::Counter { value: value2 }) if *value >= *value2 => {
                *value -= value2;
                true
            }
            (Self::Gauge { value }, Self::Gauge { value: value2 }) => {
                *value -= value2;
                true
            }
            (Self::Set { values }, Self::Set { values: values2 }) => {
                for item in values2 {
                    values.remove(item);
                }
                true
            }
            (
                Self::Distribution {
                    samples,
                    statistic: statistic_a,
                },
                Self::Distribution {
                    samples: samples2,
                    statistic: statistic_b,
                },
            ) if statistic_a == statistic_b => {
                // This is an ugly algorithm, but the use of a HashSet or equivalent is complicated by neither Hash nor
                // Eq being implemented for the f64 part of Sample.
                //
                // TODO: This logic does not work if a value is repeated within a distribution. For example, if the
                // current distribution is [1, 2, 3, 1, 2, 3] and the previous distribution is [1, 2, 3], this would
                // yield a result of [].
                //
                // The only reasonable way we could provide subtraction, I believe, is if we required the ordering to
                // stay the same, such that we would just take the samples from the non-overlapping region as the delta.
                // In the above example: length of samples from `other` would be 3, so delta would be
                // `self.samples[3..]`.
                *samples = samples
                    .iter()
                    .copied()
                    .filter(|sample| samples2.iter().all(|sample2| sample != sample2))
                    .collect();
                true
            }
            // Aggregated histograms, at least in Prometheus, are also typically monotonic in terms of growth.
            // Subtracting them in reverse -- e.g.. subtracting a newer one with more values from an older one with
            // fewer values -- would not make sense, since buckets should never be able to have negative counts... and
            // it's not clear that a saturating subtraction is technically correct either.  Instead, we avoid having to
            // make that decision, and simply force the metric to be reinitialized.
            //
            // We also check that each individual bucket count is >= the corresponding count in the
            // other histogram, since bucket value redistribution (e.g., after a source restart or
            // cache eviction) can cause individual buckets to have lower counts even when the total
            // count is higher. Failing here leads to the metric being reinitialized.
            (
                Self::AggregatedHistogram {
                    buckets,
                    count,
                    sum,
                },
                Self::AggregatedHistogram {
                    buckets: buckets2,
                    count: count2,
                    sum: sum2,
                },
            ) if *count >= *count2
                && buckets.len() == buckets2.len()
                && buckets
                    .iter()
                    .zip(buckets2.iter())
                    .all(|(b1, b2)| b1.upper_limit == b2.upper_limit && b1.count >= b2.count) =>
            {
                for (b1, b2) in buckets.iter_mut().zip(buckets2) {
                    b1.count -= b2.count;
                }
                *count -= count2;
                *sum -= sum2;
                true
            }
            _ => false,
        }
    }
}

impl ByteSizeOf for MetricValue {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Counter { .. } | Self::Gauge { .. } => 0,
            Self::Set { values } => values.allocated_bytes(),
            Self::Distribution { samples, .. } => samples.allocated_bytes(),
            Self::AggregatedHistogram { buckets, .. } => buckets.allocated_bytes(),
            Self::AggregatedSummary { quantiles, .. } => quantiles.allocated_bytes(),
            Self::Sketch { sketch } => sketch.allocated_bytes(),
            Self::NativeHistogram {
                positive_spans,
                positive_buckets,
                negative_spans,
                negative_buckets,
                ..
            } => {
                positive_spans.allocated_bytes()
                    + positive_buckets.allocated_bytes()
                    + negative_spans.allocated_bytes()
                    + negative_buckets.allocated_bytes()
            }
        }
    }
}

impl PartialEq for MetricValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Counter { value: l_value }, Self::Counter { value: r_value })
            | (Self::Gauge { value: l_value }, Self::Gauge { value: r_value }) => {
                float_eq(*l_value, *r_value)
            }
            (Self::Set { values: l_values }, Self::Set { values: r_values }) => {
                l_values == r_values
            }
            (
                Self::Distribution {
                    samples: l_samples,
                    statistic: l_statistic,
                },
                Self::Distribution {
                    samples: r_samples,
                    statistic: r_statistic,
                },
            ) => l_samples == r_samples && l_statistic == r_statistic,
            (
                Self::AggregatedHistogram {
                    buckets: l_buckets,
                    count: l_count,
                    sum: l_sum,
                },
                Self::AggregatedHistogram {
                    buckets: r_buckets,
                    count: r_count,
                    sum: r_sum,
                },
            ) => l_buckets == r_buckets && l_count == r_count && float_eq(*l_sum, *r_sum),
            (
                Self::AggregatedSummary {
                    quantiles: l_quantiles,
                    count: l_count,
                    sum: l_sum,
                },
                Self::AggregatedSummary {
                    quantiles: r_quantiles,
                    count: r_count,
                    sum: r_sum,
                },
            ) => l_quantiles == r_quantiles && l_count == r_count && float_eq(*l_sum, *r_sum),
            (Self::Sketch { sketch: l_sketch }, Self::Sketch { sketch: r_sketch }) => {
                l_sketch == r_sketch
            }
            (
                Self::NativeHistogram {
                    count: left_count,
                    sum: left_sum,
                    schema: left_schema,
                    zero_threshold: left_zero_threshold,
                    zero_count: left_zero_count,
                    positive_spans: left_positive_spans,
                    positive_buckets: left_positive_buckets,
                    negative_spans: left_negative_spans,
                    negative_buckets: left_negative_buckets,
                    reset_hint: left_reset_hint,
                },
                Self::NativeHistogram {
                    count: right_count,
                    sum: right_sum,
                    schema: right_schema,
                    zero_threshold: right_zero_threshold,
                    zero_count: right_zero_count,
                    positive_spans: right_positive_spans,
                    positive_buckets: right_positive_buckets,
                    negative_spans: right_negative_spans,
                    negative_buckets: right_negative_buckets,
                    reset_hint: right_reset_hint,
                },
            ) => {
                left_count == right_count
                    && float_eq(*left_sum, *right_sum)
                    && left_schema == right_schema
                    && float_eq(*left_zero_threshold, *right_zero_threshold)
                    && left_zero_count == right_zero_count
                    && left_positive_spans == right_positive_spans
                    && left_positive_buckets == right_positive_buckets
                    && left_negative_spans == right_negative_spans
                    && left_negative_buckets == right_negative_buckets
                    && left_reset_hint == right_reset_hint
            }
            _ => false,
        }
    }
}

impl fmt::Display for MetricValue {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            MetricValue::Counter { value } | MetricValue::Gauge { value } => {
                write!(fmt, "{value}")
            }
            MetricValue::Set { values } => {
                write_list(fmt, " ", values.iter(), |fmt, value| write_word(fmt, value))
            }
            MetricValue::Distribution { samples, statistic } => {
                write!(
                    fmt,
                    "{} ",
                    match statistic {
                        StatisticKind::Histogram => "histogram",
                        StatisticKind::Summary => "summary",
                    }
                )?;
                write_list(fmt, " ", samples, |fmt, sample| {
                    write!(fmt, "{}@{}", sample.rate, sample.value)
                })
            }
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                write!(fmt, "count={count} sum={sum} ")?;
                write_list(fmt, " ", buckets, |fmt, bucket| {
                    write!(fmt, "{}@{}", bucket.count, bucket.upper_limit)
                })
            }
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                write!(fmt, "count={count} sum={sum} ")?;
                write_list(fmt, " ", quantiles, |fmt, quantile| {
                    write!(fmt, "{}@{}", quantile.quantile, quantile.value)
                })
            }
            MetricValue::Sketch { sketch } => {
                let quantiles = [0.5, 0.75, 0.9, 0.99]
                    .iter()
                    .map(|q| Quantile {
                        quantile: *q,
                        value: 0.0,
                    })
                    .collect::<Vec<_>>();

                match sketch {
                    MetricSketch::AgentDDSketch(ddsketch) => {
                        write!(
                            fmt,
                            "count={} sum={:?} min={:?} max={:?} avg={:?} ",
                            ddsketch.count(),
                            ddsketch.sum(),
                            ddsketch.min(),
                            ddsketch.max(),
                            ddsketch.avg()
                        )?;
                        write_list(fmt, " ", quantiles, |fmt, q| {
                            write!(
                                fmt,
                                "{}={:?}",
                                q.to_percentile_string(),
                                ddsketch.quantile(q.quantile)
                            )
                        })
                    }
                }
            }
            MetricValue::NativeHistogram {
                count,
                sum,
                schema,
                zero_threshold,
                zero_count,
                positive_buckets,
                negative_buckets,
                ..
            } => {
                write!(
                    fmt,
                    "count={} sum={} schema={} zero_threshold={} zero_count={} pos_buckets={} neg_buckets={}",
                    count.as_f64(),
                    sum,
                    schema,
                    zero_threshold,
                    zero_count.as_f64(),
                    positive_buckets.len(),
                    negative_buckets.len(),
                )
            }
        }
    }
}

impl From<AgentDDSketch> for MetricValue {
    fn from(ddsketch: AgentDDSketch) -> Self {
        MetricValue::Sketch {
            sketch: MetricSketch::AgentDDSketch(ddsketch),
        }
    }
}

// Currently, VRL can only read the type of the value and doesn't consider any actual metric values.
#[cfg(feature = "vrl")]
impl From<MetricValue> for vrl::value::Value {
    fn from(value: MetricValue) -> Self {
        value.as_name().into()
    }
}

/// Type of statistics to generate for a distribution.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum StatisticKind {
    /// A histogram representation.
    Histogram,

    /// Corresponds to Datadog's Distribution Metric
    /// <https://docs.datadoghq.com/developers/metrics/types/?tab=distribution#definition>
    Summary,
}

/// A generalized metrics sketch.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetricSketch {
    /// [DDSketch][ddsketch] implementation based on the [Datadog Agent][ddagent].
    ///
    /// While `DDSketch` has open-source implementations based on the white paper, the version used in
    /// the Datadog Agent itself is subtly different. This version is suitable for sending directly
    /// to Datadog's sketch ingest endpoint.
    ///
    /// [ddsketch]: https://www.vldb.org/pvldb/vol12/p2195-masson.pdf
    /// [ddagent]: https://github.com/DataDog/datadog-agent
    AgentDDSketch(AgentDDSketch),
}

impl MetricSketch {
    /// Returns `true` if the sketch is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            MetricSketch::AgentDDSketch(ddsketch) => ddsketch.is_empty(),
        }
    }

    /// Gets the name of the sketch as a string.
    ///
    /// This maps to the name of the enum variant itself.
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::AgentDDSketch(_) => "agent dd sketch",
        }
    }
}

impl ByteSizeOf for MetricSketch {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::AgentDDSketch(ddsketch) => ddsketch.allocated_bytes(),
        }
    }
}

// Currently, VRL can only read the type of the value and doesn't consider ny actual metric values.
#[cfg(feature = "vrl")]
impl From<MetricSketch> for vrl::value::Value {
    fn from(value: MetricSketch) -> Self {
        value.as_name().into()
    }
}

/// A single observation.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// The value of the observation.
    pub value: f64,

    /// The rate at which the value was observed.
    pub rate: u32,
}

impl PartialEq for Sample {
    fn eq(&self, other: &Self) -> bool {
        self.rate == other.rate && float_eq(self.value, other.value)
    }
}

impl ByteSizeOf for Sample {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Custom serialization function which converts special `f64` values to strings.
/// Non-special values are serialized as numbers.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn serialize_f64<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if value.is_infinite() {
        serializer.serialize_str(if *value > 0.0 { INFINITY } else { NEG_INFINITY })
    } else if value.is_nan() {
        serializer.serialize_str(NAN)
    } else {
        serializer.serialize_f64(*value)
    }
}

/// Custom deserialization function for handling special f64 values.
fn deserialize_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    struct UpperLimitVisitor;

    impl de::Visitor<'_> for UpperLimitVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a number or a special string value")
        }

        fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            match value {
                NAN => Ok(f64::NAN),
                INFINITY => Ok(f64::INFINITY),
                NEG_INFINITY => Ok(f64::NEG_INFINITY),
                _ => Err(E::custom("unsupported string value")),
            }
        }
    }

    deserializer.deserialize_any(UpperLimitVisitor)
}

/// A histogram bucket.
///
/// Histogram buckets represent the `count` of observations where the value of the observations does
/// not exceed the specified `upper_limit`.
#[configurable_component(no_deser, no_ser)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Bucket {
    /// The upper limit of values in the bucket.
    #[serde(serialize_with = "serialize_f64", deserialize_with = "deserialize_f64")]
    pub upper_limit: f64,

    /// The number of values tracked in this bucket.
    pub count: u64,
}

impl PartialEq for Bucket {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count && float_eq(self.upper_limit, other.upper_limit)
    }
}

impl ByteSizeOf for Bucket {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// A single quantile observation.
///
/// Quantiles themselves are "cut points dividing the range of a probability distribution into
/// continuous intervals with equal probabilities". [[1][quantiles_wikipedia]].
///
/// We use quantiles to measure the value along these probability distributions for representing
/// client-side aggregations of distributions, which represent a collection of observations over a
/// specific time window.
///
/// In general, we typically use the term "quantile" to represent the concept of _percentiles_,
/// which deal with whole integers -- 0, 1, 2, .., 99, 100 -- even though quantiles are
/// floating-point numbers and can represent higher-precision cut points, such as 0.9999, or the
/// 99.99th percentile.
///
/// [quantiles_wikipedia]: https://en.wikipedia.org/wiki/Quantile
#[configurable_component]
#[derive(Clone, Copy, Debug)]
pub struct Quantile {
    /// The value of the quantile.
    ///
    /// This value must be between 0.0 and 1.0, inclusive.
    pub quantile: f64,

    /// The estimated value of the given quantile within the probability distribution.
    pub value: f64,
}

impl PartialEq for Quantile {
    fn eq(&self, other: &Self) -> bool {
        float_eq(self.quantile, other.quantile) && float_eq(self.value, other.value)
    }
}

impl Quantile {
    /// Renders this quantile as a string, scaled to be a percentile.
    ///
    /// Up to four significant digits are maintained, but the resulting string will be without a decimal point.
    ///
    /// For example, a quantile of 0.25, which represents a percentile of 25, will be rendered as "25" and a quantile of
    /// 0.9999, which represents a percentile of 99.99, will be rendered as "9999". A quantile of 0.99999, which
    /// represents a percentile of 99.999, would also be rendered as "9999", though.
    pub fn to_percentile_string(&self) -> String {
        let clamped = self.quantile.clamp(0.0, 1.0) * 100.0;
        clamped
            .to_string()
            .chars()
            .take(5)
            .filter(|c| *c != '.')
            .collect()
    }

    /// Renders this quantile as a string.
    ///
    /// Up to four significant digits are maintained.
    ///
    /// For example, a quantile of 0.25 will be rendered as "0.25", and a quantile of 0.9999 will be rendered as
    /// "0.9999", but a quantile of 0.99999 will be rendered as "0.9999".
    pub fn to_quantile_string(&self) -> String {
        let clamped = self.quantile.clamp(0.0, 1.0);
        clamped.to_string().chars().take(6).collect()
    }
}

impl ByteSizeOf for Quantile {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Count type for native histograms.
///
/// Integer counts are used for traditional counter-style histograms. Float counts are used for gauge histograms where
/// values may decrease, or when the source uses float counts.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NativeHistogramCount {
    /// Integer count value.
    Integer(u64),

    /// Floating-point count value.
    Float(f64),
}

impl NativeHistogramCount {
    /// Returns this count as a floating point value.
    pub fn as_f64(&self) -> f64 {
        match self {
            // The loss of precision here is acceptable for display/summary purposes.
            #[allow(clippy::cast_precision_loss)]
            Self::Integer(v) => *v as f64,
            Self::Float(v) => *v,
        }
    }

    /// Returns `true` if the count represents a float-type histogram (gauge histogram).
    pub const fn is_float(&self) -> bool {
        matches!(self, Self::Float(_))
    }
}

impl PartialEq for NativeHistogramCount {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => float_eq(*a, *b),
            _ => false,
        }
    }
}

impl Default for NativeHistogramCount {
    fn default() -> Self {
        Self::Integer(0)
    }
}

impl ByteSizeOf for NativeHistogramCount {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// A span of consecutive populated buckets in a native histogram.
///
/// Native histograms use a sparse representation: rather than storing every bucket, only non-empty ranges ("spans") are
/// stored. A span indicates where a run of consecutive buckets begins and how long it is.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHistogramSpan {
    /// Gap in bucket indices from the previous span (or from zero for the first span).
    pub offset: i32,

    /// Number of consecutive buckets in this span.
    pub length: u32,
}

impl ByteSizeOf for NativeHistogramSpan {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Bucket counts for native histograms.
///
/// Integer histograms store bucket counts as deltas from the previous bucket (first value is absolute), enabling
/// efficient encoding. Float histograms (gauge histograms) store absolute bucket counts directly.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum NativeHistogramBuckets {
    /// Delta-encoded integer bucket counts.
    ///
    /// The first value is the absolute count of the first bucket; subsequent values are the delta from the previous
    /// bucket's count.
    IntegerDeltas(Vec<i64>),

    /// Absolute floating-point bucket counts.
    FloatCounts(Vec<f64>),
}

impl NativeHistogramBuckets {
    /// Returns the number of buckets represented.
    pub fn len(&self) -> usize {
        match self {
            Self::IntegerDeltas(v) => v.len(),
            Self::FloatCounts(v) => v.len(),
        }
    }

    /// Returns `true` if there are no buckets.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterates over absolute bucket counts as floating-point values.
    ///
    /// For integer-delta buckets, this decodes the deltas into absolute counts.
    pub fn iter_absolute(&self) -> impl Iterator<Item = f64> + '_ {
        let mut running: i64 = 0;
        let mut idx: usize = 0;
        std::iter::from_fn(move || match self {
            Self::IntegerDeltas(deltas) => {
                let d = *deltas.get(idx)?;
                running = running.saturating_add(d);
                idx += 1;
                // Allow precision loss for display/summary purposes.
                #[allow(clippy::cast_precision_loss)]
                Some(running as f64)
            }
            Self::FloatCounts(counts) => {
                let v = *counts.get(idx)?;
                idx += 1;
                Some(v)
            }
        })
    }
}

impl PartialEq for NativeHistogramBuckets {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IntegerDeltas(a), Self::IntegerDeltas(b)) => a == b,
            (Self::FloatCounts(a), Self::FloatCounts(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| float_eq(*x, *y))
            }
            _ => false,
        }
    }
}

impl Default for NativeHistogramBuckets {
    fn default() -> Self {
        Self::IntegerDeltas(Vec::new())
    }
}

impl ByteSizeOf for NativeHistogramBuckets {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::IntegerDeltas(v) => v.allocated_bytes(),
            Self::FloatCounts(v) => v.allocated_bytes(),
        }
    }
}

/// Reset hint for native histograms, indicating whether the histogram was reset.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeHistogramResetHint {
    /// No hint; receiver should detect resets from the data.
    #[default]
    Unknown,

    /// This histogram is the first after a reset (or the very first observation).
    Yes,

    /// This histogram is known not to be the first after a reset.
    No,

    /// This histogram is a gauge histogram (no reset semantics).
    Gauge,
}

impl ByteSizeOf for NativeHistogramResetHint {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// Compute the upper bound of a native histogram bucket given its index and schema.
///
/// For positive indices, the upper bound is `(2^(2^-schema))^index`. For index 0, the upper bound is 1.0.
/// The lower bound of a bucket is the upper bound of the previous bucket (or `zero_threshold` for the first positive
/// bucket).
#[must_use]
pub fn native_histogram_bucket_upper_bound(schema: i32, index: i32) -> f64 {
    // Special case: schema -4 through 8, index can be negative or positive.
    // upper_bound = 2^(index * 2^(-schema))
    let exp = f64::from(index) * (-f64::from(schema)).exp2();
    exp.exp2()
}

/// Iterate over `(bucket_index, absolute_count)` pairs for the given spans and bucket data.
///
/// Spans describe which bucket indices are populated; this function zips them with the absolute (decoded) counts from
/// the bucket storage.
fn iter_span_counts<'a>(
    spans: &'a [NativeHistogramSpan],
    buckets: &'a NativeHistogramBuckets,
) -> impl Iterator<Item = (i32, f64)> + 'a {
    // First, expand spans into a flat sequence of bucket indices.
    let indices = spans.iter().scan(0i32, |index, span| {
        *index += span.offset;
        let start = *index;
        #[allow(clippy::cast_possible_wrap)]
        {
            *index += span.length as i32;
        }
        Some((start, span.length))
    });

    indices
        .flat_map(|(start, length)| {
            #[allow(clippy::cast_possible_wrap)]
            (0..length).map(move |i| start + i as i32)
        })
        .zip(buckets.iter_absolute())
}

#[cfg(test)]
mod native_histogram_tests {
    use super::*;

    #[test]
    fn bucket_upper_bound_schema_0() {
        // Schema 0: bucket boundaries are powers of 2.
        assert_eq!(native_histogram_bucket_upper_bound(0, 0), 1.0);
        assert_eq!(native_histogram_bucket_upper_bound(0, 1), 2.0);
        assert_eq!(native_histogram_bucket_upper_bound(0, 2), 4.0);
        assert_eq!(native_histogram_bucket_upper_bound(0, 3), 8.0);
        assert_eq!(native_histogram_bucket_upper_bound(0, -1), 0.5);
    }

    #[test]
    fn bucket_upper_bound_schema_1() {
        // Schema 1: bucket boundaries at 2^(n/2), so sqrt(2)^n.
        assert_eq!(native_histogram_bucket_upper_bound(1, 0), 1.0);
        assert!((native_histogram_bucket_upper_bound(1, 1) - 2.0_f64.sqrt()).abs() < 1e-10);
        assert!((native_histogram_bucket_upper_bound(1, 2) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn iter_absolute_decodes_integer_deltas() {
        let buckets = NativeHistogramBuckets::IntegerDeltas(vec![2, 1, -2, 3]);
        let absolute: Vec<f64> = buckets.iter_absolute().collect();
        assert_eq!(absolute, vec![2.0, 3.0, 1.0, 4.0]);
    }

    #[test]
    fn iter_absolute_passes_float_counts() {
        let buckets = NativeHistogramBuckets::FloatCounts(vec![1.5, 2.5, 0.5]);
        let absolute: Vec<f64> = buckets.iter_absolute().collect();
        assert_eq!(absolute, vec![1.5, 2.5, 0.5]);
    }

    #[test]
    fn iter_span_counts_single_span() {
        let spans = vec![NativeHistogramSpan {
            offset: 1,
            length: 3,
        }];
        let buckets = NativeHistogramBuckets::IntegerDeltas(vec![2, 1, -2]);
        let result: Vec<(i32, f64)> = iter_span_counts(&spans, &buckets).collect();
        assert_eq!(result, vec![(1, 2.0), (2, 3.0), (3, 1.0)]);
    }

    #[test]
    fn iter_span_counts_multiple_spans_with_gap() {
        // Span 1: indices 0..2, Span 2: gap of 3, then indices 5..7
        let spans = vec![
            NativeHistogramSpan {
                offset: 0,
                length: 2,
            },
            NativeHistogramSpan {
                offset: 3,
                length: 2,
            },
        ];
        let buckets = NativeHistogramBuckets::IntegerDeltas(vec![1, 1, 1, 1]);
        let result: Vec<(i32, f64)> = iter_span_counts(&spans, &buckets).collect();
        assert_eq!(result, vec![(0, 1.0), (1, 2.0), (5, 3.0), (6, 4.0)]);
    }

    #[test]
    fn native_histogram_to_agg_histogram_basic() {
        let native = MetricValue::NativeHistogram {
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
        };

        let agg = native.native_histogram_to_agg_histogram().unwrap();
        match agg {
            MetricValue::AggregatedHistogram {
                buckets,
                count,
                sum,
            } => {
                assert_eq!(count, 6);
                assert_eq!(sum, 18.5);
                // Buckets at indices 1, 2, 3 with schema 0 -> upper bounds 2.0, 4.0, 8.0
                // Counts: 2, 3, 1
                assert_eq!(buckets.len(), 3);
                assert_eq!(buckets[0].upper_limit, 2.0);
                assert_eq!(buckets[0].count, 2);
                assert_eq!(buckets[1].upper_limit, 4.0);
                assert_eq!(buckets[1].count, 3);
                assert_eq!(buckets[2].upper_limit, 8.0);
                assert_eq!(buckets[2].count, 1);
            }
            _ => panic!("expected AggregatedHistogram"),
        }
    }

    #[test]
    fn native_histogram_to_agg_histogram_with_zero_bucket() {
        let native = MetricValue::NativeHistogram {
            count: NativeHistogramCount::Integer(5),
            sum: 3.0,
            schema: 0,
            zero_threshold: 0.001,
            zero_count: NativeHistogramCount::Integer(2),
            positive_spans: vec![NativeHistogramSpan {
                offset: 1,
                length: 1,
            }],
            positive_buckets: NativeHistogramBuckets::IntegerDeltas(vec![3]),
            negative_spans: vec![],
            negative_buckets: NativeHistogramBuckets::IntegerDeltas(vec![]),
            reset_hint: NativeHistogramResetHint::Unknown,
        };

        let agg = native.native_histogram_to_agg_histogram().unwrap();
        match agg {
            MetricValue::AggregatedHistogram { buckets, count, .. } => {
                assert_eq!(count, 5);
                assert_eq!(buckets.len(), 2);
                // Zero bucket at threshold 0.001
                assert_eq!(buckets[0].upper_limit, 0.001);
                assert_eq!(buckets[0].count, 2);
                // Positive bucket at index 1, schema 0 -> 2.0
                assert_eq!(buckets[1].upper_limit, 2.0);
                assert_eq!(buckets[1].count, 3);
            }
            _ => panic!("expected AggregatedHistogram"),
        }
    }

    #[test]
    fn native_histogram_is_empty() {
        let empty = MetricValue::NativeHistogram {
            count: NativeHistogramCount::Integer(0),
            sum: 0.0,
            schema: 0,
            zero_threshold: 0.0,
            zero_count: NativeHistogramCount::Integer(0),
            positive_spans: vec![],
            positive_buckets: NativeHistogramBuckets::IntegerDeltas(vec![]),
            negative_spans: vec![],
            negative_buckets: NativeHistogramBuckets::IntegerDeltas(vec![]),
            reset_hint: NativeHistogramResetHint::Unknown,
        };
        assert!(empty.is_empty());

        let non_empty = MetricValue::NativeHistogram {
            count: NativeHistogramCount::Integer(1),
            sum: 1.0,
            schema: 0,
            zero_threshold: 0.0,
            zero_count: NativeHistogramCount::Integer(0),
            positive_spans: vec![NativeHistogramSpan {
                offset: 0,
                length: 1,
            }],
            positive_buckets: NativeHistogramBuckets::IntegerDeltas(vec![1]),
            negative_spans: vec![],
            negative_buckets: NativeHistogramBuckets::IntegerDeltas(vec![]),
            reset_hint: NativeHistogramResetHint::Unknown,
        };
        assert!(!non_empty.is_empty());
    }
}

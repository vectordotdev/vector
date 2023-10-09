use core::fmt;
use std::collections::BTreeSet;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use vector_common::byte_size_of::ByteSizeOf;
use vector_config::configurable_component;

use crate::{float_eq, metrics::AgentDDSketch};

use super::{samples_to_buckets, write_list, write_word};

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
            (Self::Counter { ref mut value }, Self::Counter { value: value2 })
            | (Self::Gauge { ref mut value }, Self::Gauge { value: value2 }) => {
                *value += value2;
                true
            }
            (Self::Set { ref mut values }, Self::Set { values: values2 }) => {
                values.extend(values2.iter().map(Into::into));
                true
            }
            (
                Self::Distribution {
                    ref mut samples,
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
                    ref mut buckets,
                    ref mut count,
                    ref mut sum,
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
            (Self::Counter { ref mut value }, Self::Counter { value: value2 })
                if *value >= *value2 =>
            {
                *value -= value2;
                true
            }
            (Self::Gauge { ref mut value }, Self::Gauge { value: value2 }) => {
                *value -= value2;
                true
            }
            (Self::Set { ref mut values }, Self::Set { values: values2 }) => {
                for item in values2 {
                    values.remove(item);
                }
                true
            }
            (
                Self::Distribution {
                    ref mut samples,
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
            (
                Self::AggregatedHistogram {
                    ref mut buckets,
                    ref mut count,
                    ref mut sum,
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
                    .all(|(b1, b2)| b1.upper_limit == b2.upper_limit) =>
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
    /// While DDSketch has open-source implementations based on the white paper, the version used in
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

    impl<'de> de::Visitor<'de> for UpperLimitVisitor {
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

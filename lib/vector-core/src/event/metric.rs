#[cfg(feature = "vrl")]
use std::convert::TryFrom;
use std::{
    collections::{btree_map, BTreeMap, BTreeSet},
    convert::AsRef,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use float_eq::FloatEq;
use getset::{Getters, MutGetters};
use serde::{Deserialize, Serialize};
use vector_common::EventDataEq;

use crate::{
    event::{BatchNotifier, EventFinalizer, EventFinalizers, EventMetadata, Finalizable},
    metrics::{AgentDDSketch, Handle},
    ByteSizeOf,
};

#[derive(Clone, Debug, Deserialize, Getters, MutGetters, PartialEq, PartialOrd, Serialize)]
pub struct Metric {
    #[getset(get = "pub")]
    #[serde(flatten)]
    pub(super) series: MetricSeries,

    #[getset(get = "pub", get_mut = "pub")]
    #[serde(flatten)]
    pub(super) data: MetricData,

    #[getset(get = "pub", get_mut = "pub")]
    #[serde(skip_serializing, default = "EventMetadata::default")]
    metadata: EventMetadata,
}

impl ByteSizeOf for Metric {
    fn allocated_bytes(&self) -> usize {
        self.series.allocated_bytes()
            + self.data.allocated_bytes()
            + self.metadata.allocated_bytes()
    }
}

impl Finalizable for Metric {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metadata.take_finalizers()
    }
}

impl AsRef<MetricData> for Metric {
    fn as_ref(&self) -> &MetricData {
        &self.data
    }
}

impl AsRef<MetricValue> for Metric {
    fn as_ref(&self) -> &MetricValue {
        &self.data.value
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize)]
pub struct MetricSeries {
    #[serde(flatten)]
    pub name: MetricName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<MetricTags>,
}

pub type MetricTags = BTreeMap<String, String>;

impl ByteSizeOf for MetricSeries {
    fn allocated_bytes(&self) -> usize {
        self.name.allocated_bytes() + self.tags.allocated_bytes()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize)]
pub struct MetricName {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl ByteSizeOf for MetricName {
    fn allocated_bytes(&self) -> usize {
        self.name.allocated_bytes() + self.namespace.allocated_bytes()
    }
}

#[derive(Clone, Debug, Deserialize, Getters, MutGetters, PartialEq, Serialize)]
pub struct MetricData {
    #[getset(get = "pub")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,

    #[getset(get = "pub")]
    pub kind: MetricKind,

    #[getset(get = "pub", get_mut = "pub")]
    #[serde(flatten)]
    pub value: MetricValue,
}

impl PartialOrd for MetricData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.timestamp.partial_cmp(&other.timestamp)
    }
}

impl ByteSizeOf for MetricData {
    fn allocated_bytes(&self) -> usize {
        self.value.allocated_bytes()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
/// A metric may be an incremental value, updating the previous value of
/// the metric, or absolute, which sets the reference for future
/// increments.
pub enum MetricKind {
    Incremental,
    Absolute,
}

#[cfg(feature = "vrl")]
impl TryFrom<vrl_core::Value> for MetricKind {
    type Error = String;

    fn try_from(value: vrl_core::Value) -> Result<Self, Self::Error> {
        let value = value.try_bytes().map_err(|e| e.to_string())?;
        match std::str::from_utf8(&value).map_err(|e| e.to_string())? {
            "incremental" => Ok(Self::Incremental),
            "absolute" => Ok(Self::Absolute),
            value => Err(format!(
                "invalid metric kind {}, metric kind must be `absolute` or `incremental`",
                value
            )),
        }
    }
}

#[cfg(feature = "vrl")]
impl From<MetricKind> for vrl_core::Value {
    fn from(kind: MetricKind) -> Self {
        match kind {
            MetricKind::Incremental => "incremental".into(),
            MetricKind::Absolute => "absolute".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
/// A `MetricValue` is the container for the actual value of a metric.
pub enum MetricValue {
    /// A Counter is a simple value that can not decrease except to
    /// reset it to zero.
    Counter { value: f64 },
    /// A Gauge represents a sampled numerical value.
    Gauge { value: f64 },
    /// A Set contains a set of (unordered) unique values for a key.
    Set { values: BTreeSet<String> },
    /// A Distribution contains a set of sampled values.
    Distribution {
        samples: Vec<Sample>,
        statistic: StatisticKind,
    },
    /// An AggregatedHistogram contains a set of observations which are
    /// counted into buckets. It also contains the total count of all
    /// observations and their sum to allow calculating the mean.
    AggregatedHistogram {
        buckets: Vec<Bucket>,
        count: u32,
        sum: f64,
    },
    /// An AggregatedSummary contains a set of observations which are
    /// counted into a number of quantiles. Each quantile contains the
    /// upper value of the quantile (0 <= φ <= 1). It also contains the
    /// total count of all observations and their sum to allow
    /// calculating the mean.
    AggregatedSummary {
        quantiles: Vec<Quantile>,
        count: u32,
        sum: f64,
    },
    /// A Sketch represents a data structure that typically can answer questions about the
    /// cumulative distribution of the contained samples in space-efficient way.  They represent the
    /// data in a way that queries over it have bounded error guarantees without needing to hold
    /// every single sample in memory.  They are also, typically, able to be merged with other
    /// sketches of the same type such that client-side _and_ server-side aggregation can be
    /// accomplished without loss of accuracy in the queries.
    Sketch { sketch: MetricSketch },
}

impl MetricValue {
    /// Gets whether or not this value is "empty".
    ///
    /// Scalar values (counter, gauge) are never considered empty.
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

    /// Gets the name of this `MetricValue` as a string.
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
    /// Histogram bucket bounds are based on `buckets`, where the value is the upper bound of the
    /// bucket.  Samples will be thus be ordered in a "less than" fashion: if the given sample is
    /// less than or equal to a given bucket's upper bound, it will be counted towards that bucket
    /// at the given sample rate.
    ///
    /// If this `MetricValue` is not a distribution, then `None` is returned.  Otherwise,
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
    /// This conversion specifically use the `AgentDDSketch` sketch variant, in the default
    /// configuration that matches the Datadog Agent, parameter-wise.
    ///
    /// If this `MetricValue` is not a distribution, then `None` is returned.  Otherwise,
    /// `Some(MetricValue::Sketch)` is returned.
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
                l_value.eq_ulps(r_value, &1)
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
            ) => l_buckets == r_buckets && l_count == r_count && l_sum.eq_ulps(r_sum, &1),
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
            ) => l_quantiles == r_quantiles && l_count == r_count && l_sum.eq_ulps(r_sum, &1),
            (Self::Sketch { sketch: l_sketch }, Self::Sketch { sketch: r_sketch }) => {
                l_sketch == r_sketch
            }
            _ => false,
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

/// A single sample from a `MetricValue::Distribution`, containing the
/// sampled value paired with the rate at which it was observed.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, PartialOrd, Serialize)]
pub struct Sample {
    pub value: f64,
    pub rate: u32,
}

impl ByteSizeOf for Sample {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// A single value from a `MetricValue::AggregatedHistogram`. The value
/// of the bucket is the upper bound on the range of values within the
/// bucket. The lower bound on the range is just higher than the
/// previous bucket, or zero for the first bucket.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, PartialOrd, Serialize)]
pub struct Bucket {
    pub upper_limit: f64,
    pub count: u32,
}

impl ByteSizeOf for Bucket {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

/// A single value from a `MetricValue::AggregatedSummary`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, PartialOrd, Serialize)]
pub struct Quantile {
    pub quantile: f64,
    pub value: f64,
}

impl Quantile {
    /// Formats this quantile as a percentile.
    ///
    /// Up to two decimal places are maintained.  The rendered value will be without a decimal
    /// point, however.  For example, a quantile of 0.25 will be rendered as "25" and a quantile of
    /// 0.9999 will be rendered as "9999", but a quantile of 0.99999 would also be rendered as
    /// "9999".
    pub fn as_percentile(&self) -> String {
        let clamped = self.quantile.clamp(0.0, 1.0);
        let raw = format!("{}", (clamped * 100.0));
        raw.chars()
            .take(5)
            .filter(|c| c.is_numeric())
            .collect::<String>()
    }
}

impl ByteSizeOf for Quantile {
    fn allocated_bytes(&self) -> usize {
        0
    }
}

// Constructor helper macros

#[macro_export]
macro_rules! samples {
    ( $( $value:expr => $rate:expr ),* ) => {
        vec![ $( crate::event::metric::Sample { value: $value, rate: $rate }, )* ]
    }
}

#[macro_export]
macro_rules! buckets {
    ( $( $limit:expr => $count:expr ),* ) => {
        vec![ $( crate::event::metric::Bucket { upper_limit: $limit, count: $count }, )* ]
    }
}

#[macro_export]
macro_rules! quantiles {
    ( $( $q:expr => $value:expr ),* ) => {
        vec![ $( crate::event::metric::Quantile { quantile: $q, value: $value }, )* ]
    }
}

// Convenience functions for compatibility with older split-vector data types

pub fn zip_samples(
    values: impl IntoIterator<Item = f64>,
    rates: impl IntoIterator<Item = u32>,
) -> Vec<Sample> {
    values
        .into_iter()
        .zip(rates.into_iter())
        .map(|(value, rate)| Sample { value, rate })
        .collect()
}

pub fn zip_buckets(
    limits: impl IntoIterator<Item = f64>,
    counts: impl IntoIterator<Item = u32>,
) -> Vec<Bucket> {
    limits
        .into_iter()
        .zip(counts.into_iter())
        .map(|(upper_limit, count)| Bucket { upper_limit, count })
        .collect()
}

pub fn zip_quantiles(
    quantiles: impl IntoIterator<Item = f64>,
    values: impl IntoIterator<Item = f64>,
) -> Vec<Quantile> {
    quantiles
        .into_iter()
        .zip(values.into_iter())
        .map(|(quantile, value)| Quantile { quantile, value })
        .collect()
}

/// Convert the Metric value into a vrl value.
/// Currently vrl can only read the type of the value and doesn't consider
/// any actual metric values.
#[cfg(feature = "vrl")]
impl From<MetricValue> for vrl_core::Value {
    fn from(value: MetricValue) -> Self {
        value.as_name().into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatisticKind {
    Histogram,
    /// Corresponds to DataDog's Distribution Metric
    /// <https://docs.datadoghq.com/developers/metrics/types/?tab=distribution#definition>
    Summary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MetricSketch {
    /// DDSketch implementation based on the Datadog Agent.
    ///
    /// While DDSketch has open-source implementations based on the white paper, the version used in
    /// the Datadog Agent itself is subtly different.  This version is suitable for sending directly
    /// to Datadog's sketch ingest endpoint.
    AgentDDSketch(AgentDDSketch),
}

impl MetricSketch {
    /// Gets whether or not this sketch is "empty".
    pub fn is_empty(&self) -> bool {
        match self {
            MetricSketch::AgentDDSketch(ddsketch) => ddsketch.is_empty(),
        }
    }

    /// Gets the name of this `MetricSketch` as a string.
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

/// Convert the Metric sketch into a vrl value.
/// Currently vrl can only read the type of the value and doesn't consider
/// any actual metric values.
#[cfg(feature = "vrl")]
impl From<MetricSketch> for vrl_core::Value {
    fn from(value: MetricSketch) -> Self {
        value.as_name().into()
    }
}

impl Metric {
    pub fn new<T: Into<String>>(name: T, kind: MetricKind, value: MetricValue) -> Self {
        Self::new_with_metadata(name, kind, value, EventMetadata::default())
    }

    pub fn new_with_metadata<T: Into<String>>(
        name: T,
        kind: MetricKind,
        value: MetricValue,
        metadata: EventMetadata,
    ) -> Self {
        Self {
            series: MetricSeries {
                name: MetricName {
                    name: name.into(),
                    namespace: None,
                },
                tags: None,
            },
            data: MetricData {
                timestamp: None,
                kind,
                value,
            },
            metadata,
        }
    }

    #[inline]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.series.name.name = name.into();
        self
    }

    #[inline]
    pub fn with_namespace<T: Into<String>>(mut self, namespace: Option<T>) -> Self {
        self.series.name.namespace = namespace.map(Into::into);
        self
    }

    #[inline]
    pub fn with_timestamp(mut self, timestamp: Option<DateTime<Utc>>) -> Self {
        self.data.timestamp = timestamp;
        self
    }

    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.metadata.add_finalizer(finalizer);
    }

    pub fn with_batch_notifier(mut self, batch: &Arc<BatchNotifier>) -> Self {
        self.metadata = self.metadata.with_batch_notifier(batch);
        self
    }

    pub fn with_batch_notifier_option(mut self, batch: &Option<Arc<BatchNotifier>>) -> Self {
        self.metadata = self.metadata.with_batch_notifier_option(batch);
        self
    }

    #[inline]
    pub fn with_tags(mut self, tags: Option<MetricTags>) -> Self {
        self.series.tags = tags;
        self
    }

    #[inline]
    pub fn with_value(mut self, value: MetricValue) -> Self {
        self.data.value = value;
        self
    }

    #[inline]
    pub fn into_parts(self) -> (MetricSeries, MetricData, EventMetadata) {
        (self.series, self.data, self.metadata)
    }

    #[inline]
    pub fn from_parts(series: MetricSeries, data: MetricData, metadata: EventMetadata) -> Self {
        Self {
            series,
            data,
            metadata,
        }
    }

    /// Rewrite this into a Metric with the data marked as absolute.
    pub fn into_absolute(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_absolute(),
            metadata: self.metadata,
        }
    }

    /// Rewrite this into a Metric with the data marked as incremental.
    pub fn into_incremental(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_incremental(),
            metadata: self.metadata,
        }
    }

    /// Convert the `metrics_runtime::Measurement` value plus the name and
    /// labels from a Key into our internal Metric format.
    #[allow(clippy::cast_precision_loss)]
    pub fn from_metric_kv(key: &metrics::Key, handle: &Handle) -> Self {
        let value = match handle {
            Handle::Counter(counter) => MetricValue::Counter {
                // NOTE this will truncate if `counter.count()` is a value
                // greater than 2**52.
                value: counter.count() as f64,
            },
            Handle::Gauge(gauge) => MetricValue::Gauge {
                value: gauge.gauge(),
            },
            Handle::Histogram(histogram) => {
                let buckets: Vec<Bucket> = histogram
                    .buckets()
                    .map(|(upper_limit, count)| Bucket { upper_limit, count })
                    .collect();

                MetricValue::AggregatedHistogram {
                    buckets,
                    sum: histogram.sum() as f64,
                    count: histogram.count(),
                }
            }
        };

        let labels = key
            .labels()
            .map(|label| (String::from(label.key()), String::from(label.value())))
            .collect::<MetricTags>();

        Self::new(key.name().to_string(), MetricKind::Absolute, value)
            .with_namespace(Some("vector"))
            .with_timestamp(Some(Utc::now()))
            .with_tags(if labels.is_empty() {
                None
            } else {
                Some(labels)
            })
    }

    #[inline]
    pub fn name(&self) -> &str {
        &self.series.name.name
    }

    #[inline]
    pub fn namespace(&self) -> Option<&str> {
        self.series.name.namespace.as_deref()
    }

    #[inline]
    pub fn take_namespace(&mut self) -> Option<String> {
        self.series.name.namespace.take()
    }

    #[inline]
    pub fn tags(&self) -> Option<&MetricTags> {
        self.series.tags.as_ref()
    }

    #[inline]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.data.timestamp
    }

    #[inline]
    pub fn value(&self) -> &MetricValue {
        &self.data.value
    }

    #[inline]
    pub fn kind(&self) -> MetricKind {
        self.data.kind
    }

    /// Remove the tag entry for the named key, if it exists, and return
    /// the old value. *Note:* This will drop the tags map if the tag
    /// was the last entry in it.
    pub fn remove_tag(&mut self, key: &str) -> Option<String> {
        self.series.remove_tag(key)
    }

    /// Returns `true` if `name` tag is present, and matches the provided `value`
    pub fn tag_matches(&self, name: &str, value: &str) -> bool {
        self.tags()
            .filter(|t| t.get(name).filter(|v| *v == value).is_some())
            .is_some()
    }

    /// Returns the string value of a tag, if it exists
    pub fn tag_value(&self, name: &str) -> Option<String> {
        self.tags().and_then(|t| t.get(name).cloned())
    }

    /// Set or updates the string value of a tag. *Note:* This will
    /// create the tags map if it is not present.
    pub fn insert_tag(&mut self, name: String, value: String) -> Option<String> {
        self.series.insert_tag(name, value)
    }

    /// Get the tag entry for the named key. *Note:* This will create
    /// the tags map if it is not present, even if nothing is later
    /// inserted.
    pub fn tag_entry(&mut self, key: String) -> btree_map::Entry<String, String> {
        self.series.tag_entry(key)
    }

    /// Zero out the data in this metric
    pub fn zero(&mut self) {
        self.data.zero();
    }

    /// Add the data from the other metric to this one. The `other` must
    /// be incremental and contain the same value type as this one.
    #[must_use]
    pub fn add(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.add(other.as_ref())
    }

    /// Update this `MetricData` by adding the value from another.
    #[must_use]
    pub fn update(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.update(other.as_ref())
    }

    /// Subtract the data from the other metric from this one. The
    /// `other` must contain the same value type as this one.
    #[must_use]
    pub fn subtract(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.subtract(other.as_ref())
    }
}

impl EventDataEq for Metric {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.series == other.series
            && self.data == other.data
            && self.metadata.event_data_eq(&other.metadata)
    }
}

impl MetricSeries {
    /// Set or updates the string value of a tag. *Note:* This will
    /// create the tags map if it is not present.
    pub fn insert_tag(&mut self, key: String, value: String) -> Option<String> {
        (self.tags.get_or_insert_with(Default::default)).insert(key, value)
    }

    /// Remove the tag entry for the named key, if it exists, and return
    /// the old value. *Note:* This will drop the tags map if the tag
    /// was the last entry in it.
    pub fn remove_tag(&mut self, key: &str) -> Option<String> {
        match &mut self.tags {
            None => None,
            Some(tags) => {
                let result = tags.remove(key);
                if tags.is_empty() {
                    self.tags = None;
                }
                result
            }
        }
    }

    /// Get the tag entry for the named key. *Note:* This will create
    /// the tags map if it is not present, even if nothing is later
    /// inserted.
    pub fn tag_entry(&mut self, key: String) -> btree_map::Entry<String, String> {
        self.tags.get_or_insert_with(Default::default).entry(key)
    }
}

impl MetricData {
    /// Rewrite this data to mark it as absolute.
    pub fn into_absolute(self) -> Self {
        Self {
            timestamp: self.timestamp,
            kind: MetricKind::Absolute,
            value: self.value,
        }
    }

    /// Rewrite this data to mark it as incremental.
    pub fn into_incremental(self) -> Self {
        Self {
            timestamp: self.timestamp,
            kind: MetricKind::Incremental,
            value: self.value,
        }
    }

    /// Creates a new `MetricData` from individual parts.
    pub fn from_parts(
        timestamp: Option<DateTime<Utc>>,
        kind: MetricKind,
        value: MetricValue,
    ) -> Self {
        Self {
            timestamp,
            kind,
            value,
        }
    }

    /// Consumes this `MetricData` and returns its individual parts.
    pub fn into_parts(self) -> (Option<DateTime<Utc>>, MetricKind, MetricValue) {
        (self.timestamp, self.kind, self.value)
    }

    /// Update this `MetricData` by adding the value from another.
    #[must_use]
    pub fn update(&mut self, other: &Self) -> bool {
        self.value.add(&other.value) && {
            // Update the timestamp to the latest one
            self.timestamp = match (self.timestamp, other.timestamp) {
                (None, None) => None,
                (Some(t), None) | (None, Some(t)) => Some(t),
                (Some(t1), Some(t2)) => Some(t1.max(t2)),
            };
            true
        }
    }

    /// Add the data from the other metric to this one. The `other` must
    /// be incremental and contain the same value type as this one.
    #[must_use]
    pub fn add(&mut self, other: &Self) -> bool {
        other.kind == MetricKind::Incremental && self.update(other)
    }

    /// Subtract the data from the other metric from this one. The
    /// `other` must contain the same value type as this one.
    #[must_use]
    pub fn subtract(&mut self, other: &Self) -> bool {
        self.value.subtract(&other.value)
    }

    /// Zero out the data in this metric.
    pub fn zero(&mut self) {
        self.value.zero();
    }
}

impl AsRef<MetricData> for MetricData {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl MetricValue {
    /// Zero out all the values contained in this. This keeps all the
    /// bucket/value vectors for the histogram and summary metric types
    /// intact while zeroing the counts. Distribution metrics are
    /// emptied of all their values.
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

    /// Add another same value to this.
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

    /// Subtract another (same type) value from this.
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
                // This is an ugly algorithm, but the use of a HashSet
                // or equivalent is complicated by neither Hash nor Eq
                // being implemented for the f64 part of Sample.
                //
                // TODO: This logic does not work if a value is repeated within a distribution. For
                // example, if the current distribution is [1, 2, 3, 1, 2, 3] and the previous
                // distribution is [1, 2, 3], this would yield a result of [].
                //
                // The only reasonable way we could provide subtraction, I believe, is if we
                // required the ordering to stay the same, such that we would just take the samples
                // from the non-overlapping region as the delta.  In the above example: length of
                // samples from `other` would be 3, so delta would be `self.samples[3..]`.
                *samples = samples
                    .iter()
                    .copied()
                    .filter(|sample| samples2.iter().all(|sample2| sample != sample2))
                    .collect();
                true
            }
            // Aggregated histograms, at least in Prometheus, are also typically monotonic in terms
            // of growth.  Subtracting them in reverse -- e.g.. subtracting a newer one with more
            // values from an older one with fewer values -- would not make sense, since buckets
            // should never be able to have negative counts... and it's not clear that a saturating
            // subtraction is technically correct either.  Instead, we avoid having to make that
            // decision, and simply force the metric to be reinitialized.
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

impl Display for Metric {
    /// Display a metric using something like Prometheus' text format:
    ///
    /// ```text
    /// TIMESTAMP NAMESPACE_NAME{TAGS} KIND DATA
    /// ```
    ///
    /// TIMESTAMP is in ISO 8601 format with UTC time zone.
    ///
    /// KIND is either `=` for absolute metrics, or `+` for incremental
    /// metrics.
    ///
    /// DATA is dependent on the type of metric, and is a simplified
    /// representation of the data contents. In particular,
    /// distributions, histograms, and summaries are represented as a
    /// list of `X@Y` words, where `X` is the rate, count, or quantile,
    /// and `Y` is the value or bucket.
    ///
    /// example:
    /// ```text
    /// 2020-08-12T20:23:37.248661343Z vector_processed_bytes_total{component_kind="sink",component_type="blackhole"} = 6391
    /// ```
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(timestamp) = &self.data.timestamp {
            write!(fmt, "{:?} ", timestamp)?;
        }
        let kind = match self.data.kind {
            MetricKind::Absolute => '=',
            MetricKind::Incremental => '+',
        };
        self.series.fmt(fmt)?;
        write!(fmt, " {} ", kind)?;
        self.data.value.fmt(fmt)
    }
}

impl Display for MetricSeries {
    /// Display a metric series name using something like Prometheus' text format:
    ///
    /// ```text
    /// NAMESPACE_NAME{TAGS}
    /// ```
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(namespace) = &self.name.namespace {
            write_word(fmt, namespace)?;
            write!(fmt, "_")?;
        }
        write_word(fmt, &self.name.name)?;
        write!(fmt, "{{")?;
        if let Some(tags) = &self.tags {
            write_list(fmt, ",", tags.iter(), |fmt, (tag, value)| {
                write_word(fmt, tag).and_then(|()| write!(fmt, "={:?}", value))
            })?;
        }
        write!(fmt, "}}")
    }
}

impl Display for MetricValue {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        match &self {
            MetricValue::Counter { value } | MetricValue::Gauge { value } => {
                write!(fmt, "{}", value)
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
                write!(fmt, "count={} sum={} ", count, sum)?;
                write_list(fmt, " ", buckets, |fmt, bucket| {
                    write!(fmt, "{}@{}", bucket.count, bucket.upper_limit)
                })
            }
            MetricValue::AggregatedSummary {
                quantiles,
                count,
                sum,
            } => {
                write!(fmt, "count={} sum={} ", count, sum)?;
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
                                q.as_percentile(),
                                ddsketch.quantile(q.quantile)
                            )
                        })
                    }
                }
            }
        }
    }
}

fn write_list<I, T, W>(
    fmt: &mut Formatter<'_>,
    sep: &str,
    items: I,
    writer: W,
) -> Result<(), fmt::Error>
where
    I: IntoIterator<Item = T>,
    W: Fn(&mut Formatter<'_>, T) -> Result<(), fmt::Error>,
{
    let mut this_sep = "";
    for item in items {
        write!(fmt, "{}", this_sep)?;
        writer(fmt, item)?;
        this_sep = sep;
    }
    Ok(())
}

fn write_word(fmt: &mut Formatter<'_>, word: &str) -> Result<(), fmt::Error> {
    if word.contains(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        write!(fmt, "{:?}", word)
    } else {
        write!(fmt, "{}", word)
    }
}

pub fn samples_to_buckets(samples: &[Sample], buckets: &[f64]) -> (Vec<Bucket>, u32, f64) {
    let mut counts = vec![0; buckets.len()];
    let mut sum = 0.0;
    let mut count = 0;
    for sample in samples {
        buckets
            .iter()
            .enumerate()
            .skip_while(|&(_, b)| *b < sample.value)
            .for_each(|(i, _)| {
                counts[i] += sample.rate;
            });

        sum += sample.value * f64::from(sample.rate);
        count += sample.rate;
    }

    let buckets = buckets
        .iter()
        .zip(counts.iter())
        .map(|(b, c)| Bucket {
            upper_limit: *b,
            count: *c,
        })
        .collect();

    (buckets, count, sum)
}

#[cfg(test)]
mod test {
    use chrono::{offset::TimeZone, DateTime, Utc};
    use pretty_assertions::assert_eq;

    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> MetricTags {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn merge_counters() {
        let mut counter = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        );

        let delta = Metric::new(
            "counter",
            MetricKind::Incremental,
            MetricValue::Counter { value: 2.0 },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        let expected = counter
            .clone()
            .with_value(MetricValue::Counter { value: 3.0 })
            .with_timestamp(Some(ts()));

        assert!(counter.data.add(&delta.data));
        assert_eq!(counter, expected);
    }

    #[test]
    fn merge_gauges() {
        let mut gauge = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: 1.0 },
        );

        let delta = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -2.0 },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        let expected = gauge
            .clone()
            .with_value(MetricValue::Gauge { value: -1.0 })
            .with_timestamp(Some(ts()));

        assert!(gauge.data.add(&delta.data));
        assert_eq!(gauge, expected);
    }

    #[test]
    fn merge_sets() {
        let mut set = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["old".into()].into_iter().collect(),
            },
        );

        let delta = Metric::new(
            "set",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["new".into()].into_iter().collect(),
            },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        let expected = set
            .clone()
            .with_value(MetricValue::Set {
                values: vec!["old".into(), "new".into()].into_iter().collect(),
            })
            .with_timestamp(Some(ts()));

        assert!(set.data.add(&delta.data));
        assert_eq!(set, expected);
    }

    #[test]
    fn merge_histograms() {
        let mut dist = Metric::new(
            "hist",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: samples![1.0 => 10],
                statistic: StatisticKind::Histogram,
            },
        );

        let delta = Metric::new(
            "hist",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: samples![1.0 => 20],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_namespace(Some("vector"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()));

        let expected = dist
            .clone()
            .with_value(MetricValue::Distribution {
                samples: samples![1.0 => 10, 1.0 => 20],
                statistic: StatisticKind::Histogram,
            })
            .with_timestamp(Some(ts()));

        assert!(dist.data.add(&delta.data));
        assert_eq!(dist, expected);
    }

    #[test]
    fn subtract_counters() {
        // Make sure a newer/higher value counter can subtract an older/lesser value counter:
        let old_counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 4.0 },
        );

        let mut new_counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 6.0 },
        );

        assert!(new_counter.subtract(&old_counter));
        assert_eq!(new_counter.value(), &MetricValue::Counter { value: 2.0 });

        // But not the other way around:
        let old_counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 6.0 },
        );

        let mut new_reset_counter = Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        );

        assert!(!new_reset_counter.subtract(&old_counter));
    }

    #[test]
    fn subtract_aggregated_histograms() {
        // Make sure a newer/higher count aggregated histogram can subtract an older/lower count
        // aggregated histogram:
        let old_histogram = Metric::new(
            "histogram",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                count: 1,
                sum: 1.0,
                buckets: buckets!(2.0 => 1),
            },
        );

        let mut new_histogram = Metric::new(
            "histogram",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                count: 3,
                sum: 3.0,
                buckets: buckets!(2.0 => 3),
            },
        );

        assert!(new_histogram.subtract(&old_histogram));
        assert_eq!(
            new_histogram.value(),
            &MetricValue::AggregatedHistogram {
                count: 2,
                sum: 2.0,
                buckets: buckets!(2.0 => 2),
            }
        );

        // But not the other way around:
        let old_histogram = Metric::new(
            "histogram",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                count: 3,
                sum: 3.0,
                buckets: buckets!(2.0 => 3),
            },
        );

        let mut new_reset_histogram = Metric::new(
            "histogram",
            MetricKind::Absolute,
            MetricValue::AggregatedHistogram {
                count: 1,
                sum: 1.0,
                buckets: buckets!(2.0 => 1),
            },
        );

        assert!(!new_reset_histogram.subtract(&old_histogram));
    }

    #[test]
    // `too_many_lines` is mostly just useful for production code but we're not
    // able to flag the lint on only for non-test.
    #[allow(clippy::too_many_lines)]
    fn display() {
        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "one",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_tags(Some(tags()))
            ),
            r#"one{empty_tag="",normal_tag="value",true_tag="true"} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "two word",
                    MetricKind::Incremental,
                    MetricValue::Gauge { value: 2.0 }
                )
                .with_timestamp(Some(ts()))
            ),
            r#"2018-11-14T08:09:10.000000011Z "two word"{} + 2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "namespace",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_namespace(Some("vector"))
            ),
            r#"vector_namespace{} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "namespace",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.23 },
                )
                .with_namespace(Some("vector host"))
            ),
            r#""vector host"_namespace{} = 1.23"#
        );

        let mut values = BTreeSet::<String>::new();
        values.insert("v1".into());
        values.insert("v2_two".into());
        values.insert("thrəë".into());
        values.insert("four=4".into());
        assert_eq!(
            format!(
                "{}",
                Metric::new("three", MetricKind::Absolute, MetricValue::Set { values })
            ),
            r#"three{} = "four=4" "thrəë" v1 v2_two"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "four",
                    MetricKind::Absolute,
                    MetricValue::Distribution {
                        samples: samples![1.0 => 3, 2.0 => 4],
                        statistic: StatisticKind::Histogram,
                    }
                )
            ),
            r#"four{} = histogram 3@1 4@2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "five",
                    MetricKind::Absolute,
                    MetricValue::AggregatedHistogram {
                        buckets: buckets![51.0 => 53, 52.0 => 54],
                        count: 107,
                        sum: 103.0,
                    }
                )
            ),
            r#"five{} = count=107 sum=103 53@51 54@52"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric::new(
                    "six",
                    MetricKind::Absolute,
                    MetricValue::AggregatedSummary {
                        quantiles: quantiles![1.0 => 63.0, 2.0 => 64.0],
                        count: 2,
                        sum: 127.0,
                    }
                )
            ),
            r#"six{} = count=2 sum=127 1@63 2@64"#
        );
    }

    #[test]
    fn quantile_as_percentile() {
        let quantiles = [
            (-1.0, "0"),
            (0.0, "0"),
            (0.25, "25"),
            (0.50, "50"),
            (0.999, "999"),
            (0.9999, "9999"),
            (0.99999, "9999"),
            (1.0, "100"),
            (3.0, "100"),
        ];

        for (quantile, expected) in quantiles {
            let quantile = Quantile {
                quantile,
                value: 1.0,
            };
            let result = quantile.as_percentile();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn value_conversions() {
        let counter_value = MetricValue::Counter { value: 3.13 };
        assert_eq!(counter_value.distribution_to_agg_histogram(&[1.0]), None);

        let counter_value = MetricValue::Counter { value: 3.13 };
        assert_eq!(counter_value.distribution_to_sketch(), None);

        let distrib_value = MetricValue::Distribution {
            samples: samples!(1.0 => 1),
            statistic: StatisticKind::Summary,
        };
        let converted = distrib_value.distribution_to_agg_histogram(&[1.0]);
        assert!(matches!(
            converted,
            Some(MetricValue::AggregatedHistogram { .. })
        ));

        let distrib_value = MetricValue::Distribution {
            samples: samples!(1.0 => 1),
            statistic: StatisticKind::Summary,
        };
        let converted = distrib_value.distribution_to_sketch();
        assert!(matches!(converted, Some(MetricValue::Sketch { .. })));
    }
}

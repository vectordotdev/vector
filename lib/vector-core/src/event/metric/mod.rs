#[cfg(feature = "vrl")]
use std::convert::TryFrom;

#[cfg(feature = "vrl")]
use vrl::compiler::value::VrlValueConvert;

use std::{
    convert::AsRef,
    fmt::{self, Display, Formatter},
    num::NonZeroU32,
};

use chrono::{DateTime, Utc};
use vector_common::{
    byte_size_of::ByteSizeOf,
    internal_event::{OptionalTag, TaggedEventsSent},
    json_size::JsonSize,
    request_metadata::GetEventCountTags,
    EventDataEq,
};
use vector_config::configurable_component;

use super::{
    estimated_json_encoded_size_of::EstimatedJsonEncodedSizeOf, BatchNotifier, EventFinalizer,
    EventFinalizers, EventMetadata, Finalizable,
};
use crate::config::telemetry;

#[cfg(any(test, feature = "test"))]
mod arbitrary;

mod data;
pub use self::data::*;

mod series;
pub use self::series::*;

mod tags;
pub use self::tags::*;

mod value;
pub use self::value::*;

#[macro_export]
macro_rules! metric_tags {
    () => { $crate::event::MetricTags::default() };

    ($($key:expr => $value:expr,)+) => { $crate::metric_tags!($($key => $value),+) };

    ($($key:expr => $value:expr),*) => {
        [
            $( ($key.into(), $crate::event::metric::TagValue::from($value)), )*
        ].into_iter().collect::<$crate::event::MetricTags>()
    };
}

/// A metric.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    #[serde(flatten)]
    pub(super) series: MetricSeries,

    #[serde(flatten)]
    pub(super) data: MetricData,

    /// Internal event metadata.
    #[serde(skip, default = "EventMetadata::default")]
    metadata: EventMetadata,
}

impl Metric {
    /// Creates a new `Metric` with the given `name`, `kind`, and `value`.
    pub fn new<T: Into<String>>(name: T, kind: MetricKind, value: MetricValue) -> Self {
        Self::new_with_metadata(name, kind, value, EventMetadata::default())
    }

    /// Creates a new `Metric` with the given `name`, `kind`, `value`, and `metadata`.
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
                time: MetricTime {
                    timestamp: None,
                    interval_ms: None,
                },
                kind,
                value,
            },
            metadata,
        }
    }

    /// Consumes this metric, returning it with an updated series based on the given `name`.
    #[inline]
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.series.name.name = name.into();
        self
    }

    /// Consumes this metric, returning it with an updated series based on the given `namespace`.
    #[inline]
    #[must_use]
    pub fn with_namespace<T: Into<String>>(mut self, namespace: Option<T>) -> Self {
        self.series.name.namespace = namespace.map(Into::into);
        self
    }

    /// Consumes this metric, returning it with an updated timestamp.
    #[inline]
    #[must_use]
    pub fn with_timestamp(mut self, timestamp: Option<DateTime<Utc>>) -> Self {
        self.data.time.timestamp = timestamp;
        self
    }

    /// Consumes this metric, returning it with an updated interval.
    #[inline]
    #[must_use]
    pub fn with_interval_ms(mut self, interval_ms: Option<NonZeroU32>) -> Self {
        self.data.time.interval_ms = interval_ms;
        self
    }

    pub fn add_finalizer(&mut self, finalizer: EventFinalizer) {
        self.metadata.add_finalizer(finalizer);
    }

    /// Consumes this metric, returning it with an updated set of event finalizers attached to `batch`.
    #[must_use]
    pub fn with_batch_notifier(mut self, batch: &BatchNotifier) -> Self {
        self.metadata = self.metadata.with_batch_notifier(batch);
        self
    }

    /// Consumes this metric, returning it with an optionally updated set of event finalizers attached to `batch`.
    #[must_use]
    pub fn with_batch_notifier_option(mut self, batch: &Option<BatchNotifier>) -> Self {
        self.metadata = self.metadata.with_batch_notifier_option(batch);
        self
    }

    /// Consumes this metric, returning it with an updated series based on the given `tags`.
    #[inline]
    #[must_use]
    pub fn with_tags(mut self, tags: Option<MetricTags>) -> Self {
        self.series.tags = tags;
        self
    }

    /// Consumes this metric, returning it with an updated value.
    #[inline]
    #[must_use]
    pub fn with_value(mut self, value: MetricValue) -> Self {
        self.data.value = value;
        self
    }

    /// Gets a reference to the series of this metric.
    ///
    /// The "series" is the name of the metric itself, including any tags. In other words, it is the unique identifier
    /// for a metric, although metrics of different values (counter vs gauge) may be able to co-exist in outside metrics
    /// implementations with identical series.
    pub fn series(&self) -> &MetricSeries {
        &self.series
    }

    /// Gets a reference to the data of this metric.
    pub fn data(&self) -> &MetricData {
        &self.data
    }

    /// Gets a mutable reference to the data of this metric.
    pub fn data_mut(&mut self) -> &mut MetricData {
        &mut self.data
    }

    /// Gets a reference to the metadata of this metric.
    pub fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    /// Gets a mutable reference to the metadata of this metric.
    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        &mut self.metadata
    }

    /// Gets a reference to the name of this metric.
    ///
    /// The name of the metric does not include the namespace or tags.
    #[inline]
    pub fn name(&self) -> &str {
        &self.series.name.name
    }

    /// Gets a reference to the namespace of this metric, if it exists.
    #[inline]
    pub fn namespace(&self) -> Option<&str> {
        self.series.name.namespace.as_deref()
    }

    /// Takes the namespace out of this metric, if it exists, leaving it empty.
    #[inline]
    pub fn take_namespace(&mut self) -> Option<String> {
        self.series.name.namespace.take()
    }

    /// Gets a reference to the tags of this metric, if they exist.
    #[inline]
    pub fn tags(&self) -> Option<&MetricTags> {
        self.series.tags.as_ref()
    }

    /// Gets a mutable reference to the tags of this metric, if they exist.
    #[inline]
    pub fn tags_mut(&mut self) -> Option<&mut MetricTags> {
        self.series.tags.as_mut()
    }

    /// Gets a reference to the timestamp of this metric, if it exists.
    #[inline]
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.data.time.timestamp
    }

    /// Gets a reference to the interval (in milliseconds) covered by this metric, if it exists.
    #[inline]
    pub fn interval_ms(&self) -> Option<NonZeroU32> {
        self.data.time.interval_ms
    }

    /// Gets a reference to the value of this metric.
    #[inline]
    pub fn value(&self) -> &MetricValue {
        &self.data.value
    }

    /// Gets a mutable reference to the value of this metric.
    #[inline]
    pub fn value_mut(&mut self) -> &mut MetricValue {
        &mut self.data.value
    }

    /// Gets the kind of this metric.
    #[inline]
    pub fn kind(&self) -> MetricKind {
        self.data.kind
    }

    /// Gets the time information of this metric.
    #[inline]
    pub fn time(&self) -> MetricTime {
        self.data.time
    }

    /// Decomposes a `Metric` into its individual parts.
    #[inline]
    pub fn into_parts(self) -> (MetricSeries, MetricData, EventMetadata) {
        (self.series, self.data, self.metadata)
    }

    /// Creates a `Metric` directly from the raw components of another metric.
    #[inline]
    pub fn from_parts(series: MetricSeries, data: MetricData, metadata: EventMetadata) -> Self {
        Self {
            series,
            data,
            metadata,
        }
    }

    /// Consumes this metric, returning it as an absolute metric.
    ///
    /// If the metric was already absolute, nothing is changed.
    #[must_use]
    pub fn into_absolute(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_absolute(),
            metadata: self.metadata,
        }
    }

    /// Consumes this metric, returning it as an incremental metric.
    ///
    /// If the metric was already incremental, nothing is changed.
    #[must_use]
    pub fn into_incremental(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_incremental(),
            metadata: self.metadata,
        }
    }

    /// Creates a new metric from components specific to a metric emitted by `metrics`.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn from_metric_kv(
        key: &metrics::Key,
        value: MetricValue,
        timestamp: DateTime<Utc>,
    ) -> Self {
        let labels = key
            .labels()
            .map(|label| (String::from(label.key()), String::from(label.value())))
            .collect::<MetricTags>();

        Self::new(key.name().to_string(), MetricKind::Absolute, value)
            .with_namespace(Some("vector"))
            .with_timestamp(Some(timestamp))
            .with_tags((!labels.is_empty()).then_some(labels))
    }

    /// Removes a tag from this metric, returning the value of the tag if the tag was previously in the metric.
    pub fn remove_tag(&mut self, key: &str) -> Option<String> {
        self.series.remove_tag(key)
    }

    /// Removes all the tags.
    pub fn remove_tags(&mut self) {
        self.series.remove_tags();
    }

    /// Returns `true` if `name` tag is present, and matches the provided `value`
    pub fn tag_matches(&self, name: &str, value: &str) -> bool {
        self.tags()
            .filter(|t| t.get(name).filter(|v| *v == value).is_some())
            .is_some()
    }

    /// Returns the string value of a tag, if it exists
    pub fn tag_value(&self, name: &str) -> Option<String> {
        self.tags().and_then(|t| t.get(name)).map(ToOwned::to_owned)
    }

    /// Inserts a tag into this metric.
    ///
    /// If the metric did not have this tag, `None` will be returned. Otherwise, `Some(String)` will be returned,
    /// containing the previous value of the tag.
    ///
    /// *Note:* This will create the tags map if it is not present.
    pub fn replace_tag(&mut self, name: String, value: String) -> Option<String> {
        self.series.replace_tag(name, value)
    }

    pub fn set_multi_value_tag(
        &mut self,
        name: String,
        values: impl IntoIterator<Item = TagValue>,
    ) {
        self.series.set_multi_value_tag(name, values);
    }

    /// Zeroes out the data in this metric.
    pub fn zero(&mut self) {
        self.data.zero();
    }

    /// Adds the data from the `other` metric to this one.
    ///
    /// The other metric must be incremental and contain the same value type as this one.
    #[must_use]
    pub fn add(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.add(other.as_ref())
    }

    /// Updates this metric by adding the data from `other`.
    #[must_use]
    pub fn update(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.update(other.as_ref())
    }

    /// Subtracts the data from the `other` metric from this one.
    ///
    /// The other metric must contain the same value type as this one.
    #[must_use]
    pub fn subtract(&mut self, other: impl AsRef<MetricData>) -> bool {
        self.data.subtract(other.as_ref())
    }

    /// Reduces all the tag values to their single value, discarding any for which that value would
    /// be null. If the result is empty, the tag set is dropped.
    pub fn reduce_tags_to_single(&mut self) {
        if let Some(tags) = &mut self.series.tags {
            tags.reduce_to_single();
            if tags.is_empty() {
                self.series.tags = None;
            }
        }
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
    /// 2020-08-12T20:23:37.248661343Z vector_received_bytes_total{component_kind="sink",component_type="blackhole"} = 6391
    /// ```
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(timestamp) = &self.data.time.timestamp {
            write!(fmt, "{timestamp:?} ")?;
        }
        let kind = match self.data.kind {
            MetricKind::Absolute => '=',
            MetricKind::Incremental => '+',
        };
        self.series.fmt(fmt)?;
        write!(fmt, " {kind} ")?;
        self.data.value.fmt(fmt)
    }
}

impl EventDataEq for Metric {
    fn event_data_eq(&self, other: &Self) -> bool {
        self.series == other.series
            && self.data == other.data
            && self.metadata.event_data_eq(&other.metadata)
    }
}

impl ByteSizeOf for Metric {
    fn allocated_bytes(&self) -> usize {
        self.series.allocated_bytes()
            + self.data.allocated_bytes()
            + self.metadata.allocated_bytes()
    }
}

impl EstimatedJsonEncodedSizeOf for Metric {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        // TODO: For now we're using the in-memory representation of the metric, but we'll convert
        // this to actually calculate the JSON encoded size in the near future.
        self.size_of().into()
    }
}

impl Finalizable for Metric {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metadata.take_finalizers()
    }
}

impl GetEventCountTags for Metric {
    fn get_tags(&self) -> TaggedEventsSent {
        let source = if telemetry().tags().emit_source {
            self.metadata().source_id().cloned().into()
        } else {
            OptionalTag::Ignored
        };

        // Currently there is no way to specify a tag that means the service,
        // so we will be hardcoding it to "service".
        let service = if telemetry().tags().emit_service {
            self.tags()
                .and_then(|tags| tags.get("service").map(ToString::to_string))
                .into()
        } else {
            OptionalTag::Ignored
        };

        TaggedEventsSent { source, service }
    }
}

/// Metric kind.
///
/// Metrics can be either absolute of incremental. Absolute metrics represent a sort of "last write wins" scenario,
/// where the latest absolute value seen is meant to be the actual metric value.  In contrast, and perhaps intuitively,
/// incremental metrics are meant to be additive, such that we don't know what total value of the metric is, but we know
/// that we'll be adding or subtracting the given value from it.
///
/// Generally speaking, most metrics storage systems deal with incremental updates. A notable exception is Prometheus,
/// which deals with, and expects, absolute values from clients.
#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// Incremental metric.
    Incremental,

    /// Absolute metric.
    Absolute,
}

#[cfg(feature = "vrl")]
impl TryFrom<vrl::value::Value> for MetricKind {
    type Error = String;

    fn try_from(value: vrl::value::Value) -> Result<Self, Self::Error> {
        let value = value.try_bytes().map_err(|e| e.to_string())?;
        match std::str::from_utf8(&value).map_err(|e| e.to_string())? {
            "incremental" => Ok(Self::Incremental),
            "absolute" => Ok(Self::Absolute),
            value => Err(format!(
                "invalid metric kind {value}, metric kind must be `absolute` or `incremental`"
            )),
        }
    }
}

#[cfg(feature = "vrl")]
impl From<MetricKind> for vrl::value::Value {
    fn from(kind: MetricKind) -> Self {
        match kind {
            MetricKind::Incremental => "incremental".into(),
            MetricKind::Absolute => "absolute".into(),
        }
    }
}

#[macro_export]
macro_rules! samples {
    ( $( $value:expr => $rate:expr ),* ) => {
        vec![ $( $crate::event::metric::Sample { value: $value, rate: $rate }, )* ]
    }
}

#[macro_export]
macro_rules! buckets {
    ( $( $limit:expr => $count:expr ),* ) => {
        vec![ $( $crate::event::metric::Bucket { upper_limit: $limit, count: $count }, )* ]
    }
}

#[macro_export]
macro_rules! quantiles {
    ( $( $q:expr => $value:expr ),* ) => {
        vec![ $( $crate::event::metric::Quantile { quantile: $q, value: $value }, )* ]
    }
}

#[inline]
pub(crate) fn zip_samples(
    values: impl IntoIterator<Item = f64>,
    rates: impl IntoIterator<Item = u32>,
) -> Vec<Sample> {
    values
        .into_iter()
        .zip(rates)
        .map(|(value, rate)| Sample { value, rate })
        .collect()
}

#[inline]
pub(crate) fn zip_buckets(
    limits: impl IntoIterator<Item = f64>,
    counts: impl IntoIterator<Item = u64>,
) -> Vec<Bucket> {
    limits
        .into_iter()
        .zip(counts)
        .map(|(upper_limit, count)| Bucket { upper_limit, count })
        .collect()
}

#[inline]
pub(crate) fn zip_quantiles(
    quantiles: impl IntoIterator<Item = f64>,
    values: impl IntoIterator<Item = f64>,
) -> Vec<Quantile> {
    quantiles
        .into_iter()
        .zip(values)
        .map(|(quantile, value)| Quantile { quantile, value })
        .collect()
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
        write!(fmt, "{this_sep}")?;
        writer(fmt, item)?;
        this_sep = sep;
    }
    Ok(())
}

fn write_word(fmt: &mut Formatter<'_>, word: &str) -> Result<(), fmt::Error> {
    if word.contains(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        write!(fmt, "{word:?}")
    } else {
        write!(fmt, "{word}")
    }
}

pub fn samples_to_buckets(samples: &[Sample], buckets: &[f64]) -> (Vec<Bucket>, u64, f64) {
    let mut counts = vec![0; buckets.len()];
    let mut sum = 0.0;
    let mut count = 0;
    for sample in samples {
        let rate = u64::from(sample.rate);

        if let Some((i, _)) = buckets
            .iter()
            .enumerate()
            .find(|&(_, b)| *b >= sample.value)
        {
            counts[i] += rate;
        }

        sum += sample.value * f64::from(sample.rate);
        count += rate;
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
    use std::collections::BTreeSet;

    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use similar_asserts::assert_eq;

    use super::*;

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    fn tags() -> MetricTags {
        metric_tags!(
            "normal_tag" => "value",
            "true_tag" => "true",
            "empty_tag" => "",
        )
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
            r"vector_namespace{} = 1.23"
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
            r"four{} = histogram 3@1 4@2"
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
            r"five{} = count=107 sum=103 53@51 54@52"
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
            r"six{} = count=2 sum=127 1@63 2@64"
        );
    }

    #[test]
    fn quantile_to_percentile_string() {
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
            let result = quantile.to_percentile_string();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn quantile_to_string() {
        let quantiles = [
            (-1.0, "0"),
            (0.0, "0"),
            (0.25, "0.25"),
            (0.50, "0.5"),
            (0.999, "0.999"),
            (0.9999, "0.9999"),
            (0.99999, "0.9999"),
            (1.0, "1"),
            (3.0, "1"),
        ];

        for (quantile, expected) in quantiles {
            let quantile = Quantile {
                quantile,
                value: 1.0,
            };
            let result = quantile.to_quantile_string();
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
            samples: samples!(1.0 => 10, 2.0 => 5, 5.0 => 2),
            statistic: StatisticKind::Summary,
        };
        let converted = distrib_value.distribution_to_agg_histogram(&[1.0, 5.0, 10.0]);
        assert_eq!(
            converted,
            Some(MetricValue::AggregatedHistogram {
                buckets: vec![
                    Bucket {
                        upper_limit: 1.0,
                        count: 10,
                    },
                    Bucket {
                        upper_limit: 5.0,
                        count: 7,
                    },
                    Bucket {
                        upper_limit: 10.0,
                        count: 0,
                    },
                ],
                sum: 30.0,
                count: 17,
            })
        );

        let distrib_value = MetricValue::Distribution {
            samples: samples!(1.0 => 1),
            statistic: StatisticKind::Summary,
        };
        let converted = distrib_value.distribution_to_sketch();
        assert!(matches!(converted, Some(MetricValue::Sketch { .. })));
    }

    #[test]
    fn merge_non_contiguous_interval() {
        let mut gauge = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: 12.0 },
        )
        .with_timestamp(Some(ts()))
        .with_interval_ms(std::num::NonZeroU32::new(10));

        let delta = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -5.0 },
        )
        .with_timestamp(Some(ts() + chrono::Duration::milliseconds(20)))
        .with_interval_ms(std::num::NonZeroU32::new(15));

        let expected = gauge
            .clone()
            .with_value(MetricValue::Gauge { value: 7.0 })
            .with_timestamp(Some(ts()))
            .with_interval_ms(std::num::NonZeroU32::new(35));

        assert!(gauge.data.add(&delta.data));
        assert_eq!(gauge, expected);
    }

    #[test]
    fn merge_contiguous_interval() {
        let mut gauge = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: 12.0 },
        )
        .with_timestamp(Some(ts()))
        .with_interval_ms(std::num::NonZeroU32::new(10));

        let delta = Metric::new(
            "gauge",
            MetricKind::Incremental,
            MetricValue::Gauge { value: -5.0 },
        )
        .with_timestamp(Some(ts() + chrono::Duration::milliseconds(5)))
        .with_interval_ms(std::num::NonZeroU32::new(15));

        let expected = gauge
            .clone()
            .with_value(MetricValue::Gauge { value: 7.0 })
            .with_timestamp(Some(ts()))
            .with_interval_ms(std::num::NonZeroU32::new(20));

        assert!(gauge.data.add(&delta.data));
        assert_eq!(gauge, expected);
    }
}

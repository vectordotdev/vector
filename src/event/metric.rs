use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use remap::{Object, Segment};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    fmt::{self, Display, Formatter},
    iter::FromIterator,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Metric {
    #[serde(flatten)]
    pub series: MetricSeries,
    #[serde(flatten)]
    pub data: MetricData,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MetricSeries {
    #[serde(flatten)]
    pub name: MetricName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<MetricTags>,
}

pub type MetricTags = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct MetricName {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    pub kind: MetricKind,
    #[serde(flatten)]
    pub value: MetricValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
/// A metric may be an incremental value, updating the previous value of
/// the metric, or absolute, which sets the reference for future
/// increments.
pub enum MetricKind {
    Incremental,
    Absolute,
}

impl TryFrom<remap::Value> for MetricKind {
    type Error = String;

    fn try_from(value: remap::Value) -> Result<Self, Self::Error> {
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

impl From<MetricKind> for remap::Value {
    fn from(kind: MetricKind) -> Self {
        match kind {
            MetricKind::Incremental => "incremental".into(),
            MetricKind::Absolute => "absolute".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
/// A MetricValue is the container for the actual value of a metric.
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
}

/// A single sample from a `MetricValue::Distribution`, containing the
/// sampled value paired with the rate at which it was observed.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Sample {
    pub value: f64,
    pub rate: u32,
}

/// A single value from a `MetricValue::AggregatedHistogram`. The value
/// of the bucket is the upper bound on the range of values within the
/// bucket. The lower bound on the range is just higher than the
/// previous bucket, or zero for the first bucket.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Bucket {
    pub upper_limit: f64,
    pub count: u32,
}

/// A single value from a `MetricValue::AggregatedSummary`.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Quantile {
    pub upper_limit: f64,
    pub value: f64,
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
    ( $( $limit:expr => $value:expr ),* ) => {
        vec![ $( crate::event::metric::Quantile { upper_limit: $limit, value: $value }, )* ]
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
    limits: impl IntoIterator<Item = f64>,
    values: impl IntoIterator<Item = f64>,
) -> Vec<Quantile> {
    limits
        .into_iter()
        .zip(values.into_iter())
        .map(|(upper_limit, value)| Quantile { upper_limit, value })
        .collect()
}

/// Convert the Metric value into a remap value.
/// Currently remap can only read the type of the value and doesn't consider
/// any actual metric values.
impl From<MetricValue> for remap::Value {
    fn from(value: MetricValue) -> Self {
        match value {
            MetricValue::Counter { .. } => "counter",
            MetricValue::Gauge { .. } => "gauge",
            MetricValue::Set { .. } => "set",
            MetricValue::Distribution { .. } => "distribution",
            MetricValue::AggregatedHistogram { .. } => "aggregated histogram",
            MetricValue::AggregatedSummary { .. } => "aggregated summary",
        }
        .into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
pub enum StatisticKind {
    Histogram,
    /// Corresponds to DataDog's Distribution Metric
    /// https://docs.datadoghq.com/developers/metrics/types/?tab=distribution#definition
    Summary,
}

impl Metric {
    pub fn new<T: Into<String>>(name: T, kind: MetricKind, value: MetricValue) -> Self {
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
        }
    }

    pub fn with_namespace<T: Into<String>>(mut self, namespace: Option<T>) -> Self {
        self.series.name.namespace = namespace.map(Into::into);
        self
    }

    pub fn with_timestamp(mut self, timestamp: Option<DateTime<Utc>>) -> Self {
        self.data.timestamp = timestamp;
        self
    }

    pub fn with_tags(mut self, tags: Option<MetricTags>) -> Self {
        self.series.tags = tags;
        self
    }

    /// Rewrite this into a Metric with the data marked as absolute.
    pub fn into_absolute(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_absolute(),
        }
    }

    /// Rewrite this into a Metric with the data marked as incremental.
    pub fn into_incremental(self) -> Self {
        Self {
            series: self.series,
            data: self.data.into_incremental(),
        }
    }

    /// Convert the metrics_runtime::Measurement value plus the name and
    /// labels from a Key into our internal Metric format.
    pub fn from_metric_kv(key: &metrics::Key, handle: &metrics_util::Handle) -> Self {
        let value = match handle {
            metrics_util::Handle::Counter(_) => MetricValue::Counter {
                value: handle.read_counter() as f64,
            },
            metrics_util::Handle::Gauge(_) => MetricValue::Gauge {
                value: handle.read_gauge() as f64,
            },
            metrics_util::Handle::Histogram(_) => {
                let values = handle.read_histogram();
                // Each sample in the source measurement has an
                // effective sample rate of 1.
                let samples = values
                    .into_iter()
                    .map(|i| Sample {
                        value: i as f64,
                        rate: 1,
                    })
                    .collect();
                MetricValue::Distribution {
                    samples,
                    statistic: StatisticKind::Histogram,
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

    pub fn name(&self) -> &str {
        &self.series.name.name
    }

    pub fn namespace(&self) -> Option<&str> {
        self.series.name.namespace.as_deref()
    }

    pub fn tags(&self) -> Option<&MetricTags> {
        self.series.tags.as_ref()
    }

    pub fn tags_mut(&mut self) -> &mut Option<MetricTags> {
        &mut self.series.tags
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

    /// Sets or updates the string value of a tag
    pub fn set_tag_value(&mut self, name: String, value: String) {
        self.tags_mut()
            .get_or_insert_with(MetricTags::new)
            .insert(name, value);
    }

    /// Deletes the tag, if it exists, returns the old tag value.
    pub fn delete_tag(&mut self, name: &str) -> Option<String> {
        self.series.tags.as_mut().and_then(|tags| tags.remove(name))
    }

    /// Create a new metric from this with the data zeroed.
    pub fn zero(&self) -> Self {
        Self {
            series: self.series.clone(),
            data: self.data.zero(),
        }
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

    /// Update this MetricData by adding the value from another.
    pub fn update(&mut self, other: &Self) {
        self.value.add(&other.value)
    }

    /// Add the data from the other metric to this one. The `other` must
    /// be relative and contain the same value type as this one.
    pub fn add(&mut self, other: &Self) {
        if other.kind.is_incremental() {
            self.update(other);
        }
    }

    /// Create a new metric data from this with a zero value.
    pub fn zero(&self) -> Self {
        Self {
            timestamp: self.timestamp,
            kind: self.kind,
            value: self.value.zero(),
        }
    }
}

impl MetricValue {
    /// Create a new metric value with all the contained values set to
    /// zero. This keeps all the bucket/value vectors for the histogram
    /// and summary metric types intact while zeroing the
    /// counts. Distribution metrics are emptied of all their values.
    pub fn zero(&self) -> Self {
        match self {
            Self::Counter { .. } => Self::Counter { value: 0.0 },
            Self::Gauge { .. } => Self::Gauge { value: 0.0 },
            Self::Set { .. } => Self::Set {
                values: BTreeSet::default(),
            },
            Self::Distribution { samples, statistic } => Self::Distribution {
                samples: Vec::with_capacity(samples.len()),
                statistic: *statistic,
            },
            Self::AggregatedHistogram { buckets, .. } => Self::AggregatedHistogram {
                buckets: buckets
                    .iter()
                    .map(|&Bucket { upper_limit, .. }| Bucket {
                        upper_limit,
                        count: 0,
                    })
                    .collect(),
                count: 0,
                sum: 0.0,
            },
            Self::AggregatedSummary { quantiles, .. } => Self::AggregatedSummary {
                quantiles: quantiles
                    .iter()
                    .map(|&Quantile { upper_limit, .. }| Quantile {
                        upper_limit,
                        value: 0.0,
                    })
                    .collect(),
                count: 0,
                sum: 0.0,
            },
        }
    }

    /// Add another same value to this.
    pub fn add(&mut self, other: &Self) {
        match (self, other) {
            (Self::Counter { ref mut value }, Self::Counter { value: value2 }) => {
                *value += value2;
            }
            (Self::Gauge { ref mut value }, Self::Gauge { value: value2 }) => {
                *value += value2;
            }
            (Self::Set { ref mut values }, Self::Set { values: values2 }) => {
                values.extend(values2.iter().map(Into::into));
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
                samples.extend_from_slice(&samples2);
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
            ) => {
                if buckets.len() == buckets2.len()
                    && buckets
                        .iter()
                        .zip(buckets2.iter())
                        .all(|(b1, b2)| b1.upper_limit == b2.upper_limit)
                {
                    for (b1, b2) in buckets.iter_mut().zip(buckets2) {
                        b1.count += b2.count;
                    }
                    *count += count2;
                    *sum += sum2;
                }
            }
            (
                Self::AggregatedSummary {
                    ref mut quantiles,
                    ref mut count,
                    ref mut sum,
                },
                Self::AggregatedSummary {
                    quantiles: quantiles2,
                    count: count2,
                    sum: sum2,
                },
            ) => {
                if quantiles.len() == quantiles2.len()
                    && quantiles
                        .iter()
                        .zip(quantiles2.iter())
                        .all(|(b1, b2)| b1.upper_limit == b2.upper_limit)
                {
                    for (b1, b2) in quantiles.iter_mut().zip(quantiles2) {
                        b1.value += b2.value;
                    }
                    *count += count2;
                    *sum += sum2;
                }
            }
            _ => {}
        }
    }

    /// Subtract another (same type) value from this.
    pub fn subtract(&mut self, other: &Self) {
        match (self, other) {
            (Self::Counter { ref mut value }, Self::Counter { value: value2 }) => {
                *value -= value2;
            }
            (Self::Gauge { ref mut value }, Self::Gauge { value: value2 }) => {
                *value -= value2;
            }
            (Self::Set { ref mut values }, Self::Set { values: values2 }) => {
                for item in values2 {
                    values.remove(item);
                }
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
                *samples = samples
                    .iter()
                    .copied()
                    .filter(|sample| samples2.iter().find(|sample2| sample == *sample2).is_none())
                    .collect();
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
            ) => {
                if buckets.len() == buckets2.len()
                    && buckets
                        .iter()
                        .zip(buckets2.iter())
                        .all(|(b1, b2)| b1.upper_limit == b2.upper_limit)
                {
                    for (b1, b2) in buckets.iter_mut().zip(buckets2) {
                        b1.count -= b2.count;
                    }
                    *count -= count2;
                    *sum -= sum2;
                }
            }
            (
                Self::AggregatedSummary {
                    ref mut quantiles,
                    ref mut count,
                    ref mut sum,
                },
                Self::AggregatedSummary {
                    quantiles: quantiles2,
                    count: count2,
                    sum: sum2,
                },
            ) => {
                if quantiles.len() == quantiles2.len()
                    && quantiles
                        .iter()
                        .zip(quantiles2.iter())
                        .all(|(b1, b2)| b1.upper_limit == b2.upper_limit)
                {
                    for (b1, b2) in quantiles.iter_mut().zip(quantiles2) {
                        b1.value -= b2.value;
                    }
                    *count -= count2;
                    *sum -= sum2;
                }
            }
            _ => {}
        }
    }
}

impl Display for Metric {
    /// Display a metric using something like Prometheus' text format:
    ///
    /// TIMESTAMP NAMESPACE_NAME{TAGS} KIND DATA
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
        if let Some(namespace) = &self.namespace() {
            write_word(fmt, namespace)?;
            write!(fmt, "_")?;
        }
        write_word(fmt, &self.name())?;
        write!(fmt, "{{")?;
        if let Some(tags) = &self.tags() {
            write_list(fmt, ",", tags.iter(), |fmt, (tag, value)| {
                write_word(fmt, tag).and_then(|()| write!(fmt, "={:?}", value))
            })?;
        }
        write!(
            fmt,
            "}} {} ",
            match self.data.kind {
                MetricKind::Absolute => '=',
                MetricKind::Incremental => '+',
            }
        )?;
        match &self.data.value {
            MetricValue::Counter { value } => write!(fmt, "{}", value),
            MetricValue::Gauge { value } => write!(fmt, "{}", value),
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
                    write!(fmt, "{}@{}", quantile.upper_limit, quantile.value)
                })
            }
        }
    }
}

const VALID_METRIC_PATHS_SET: &str = ".name, .namespace, .timestamp, .kind, .tags";

/// We can get the `type` of the metric in Remap, but can't set  it.
const VALID_METRIC_PATHS_GET: &str = ".name, .namespace, .timestamp, .kind, .tags, .type";

#[derive(Debug, Snafu)]
enum MetricPathError<'a> {
    #[snafu(display("cannot set root path"))]
    SetPathError,

    #[snafu(display("invalid path {}: expected one of {}", path, expected))]
    InvalidPath { path: &'a str, expected: &'a str },
}

impl Object for Metric {
    fn insert(&mut self, path: &remap::Path, value: remap::Value) -> Result<(), String> {
        if path.is_root() {
            return Err(MetricPathError::SetPathError.to_string());
        }

        match path.segments() {
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                let value = value.try_bytes().map_err(|e| e.to_string())?;
                self.set_tag_value(
                    field.as_str().to_owned(),
                    String::from_utf8_lossy(&value).into_owned(),
                );
                Ok(())
            }
            [Segment::Field(name)] if name.as_str() == "name" => {
                let value = value.try_bytes().map_err(|e| e.to_string())?;
                self.series.name.name = String::from_utf8_lossy(&value).into_owned();
                Ok(())
            }
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                let value = value.try_bytes().map_err(|e| e.to_string())?;
                self.series.name.namespace = Some(String::from_utf8_lossy(&value).into_owned());
                Ok(())
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                let value = value.try_timestamp().map_err(|e| e.to_string())?;
                self.data.timestamp = Some(value);
                Ok(())
            }
            [Segment::Field(kind)] if kind.as_str() == "kind" => {
                self.data.kind = MetricKind::try_from(value)?;
                Ok(())
            }
            _ => Err(MetricPathError::InvalidPath {
                path: &path.to_string(),
                expected: VALID_METRIC_PATHS_SET,
            }
            .to_string()),
        }
    }

    fn get(&self, path: &remap::Path) -> Result<Option<remap::Value>, String> {
        if path.is_root() {
            let mut map = BTreeMap::<String, remap::Value>::new();
            map.insert("name".to_string(), self.series.name.name.clone().into());
            if let Some(ref namespace) = self.series.name.namespace {
                map.insert("namespace".to_string(), namespace.clone().into());
            }
            if let Some(timestamp) = self.data.timestamp {
                map.insert("timestamp".to_string(), timestamp.into());
            }
            map.insert("kind".to_string(), self.data.kind.into());
            if let Some(tags) = self.tags() {
                map.insert(
                    "tags".to_string(),
                    tags.iter()
                        .map(|(tag, value)| (tag.clone(), value.clone().into()))
                        .collect::<BTreeMap<_, _>>()
                        .into(),
                );
            }
            map.insert("type".to_string(), self.data.value.clone().into());

            return Ok(Some(map.into()));
        }

        match path.segments() {
            [Segment::Field(name)] if name.as_str() == "name" => {
                Ok(Some(self.name().to_string().into()))
            }
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                Ok(self.series.name.namespace.clone().map(Into::into))
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                Ok(self.data.timestamp.map(Into::into))
            }
            [Segment::Field(kind)] if kind.as_str() == "kind" => {
                Ok(Some(self.data.kind.clone().into()))
            }
            [Segment::Field(tags)] if tags.as_str() == "tags" => Ok(self.tags().map(|map| {
                let iter = map.iter().map(|(k, v)| (k.to_owned(), v.to_owned().into()));
                remap::Value::from_iter(iter)
            })),
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                Ok(self.tag_value(field.as_str()).map(|value| value.into()))
            }
            [Segment::Field(type_)] if type_.as_str() == "type" => {
                Ok(Some(self.data.value.clone().into()))
            }
            _ => Err(MetricPathError::InvalidPath {
                path: &path.to_string(),
                expected: VALID_METRIC_PATHS_GET,
            }
            .to_string()),
        }
    }

    fn remove(
        &mut self,
        path: &remap::Path,
        _compact: bool,
    ) -> Result<Option<remap::Value>, String> {
        if path.is_root() {
            return Err(MetricPathError::SetPathError.to_string());
        }

        match path.segments() {
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                Ok(self.series.name.namespace.take().map(Into::into))
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                Ok(self.data.timestamp.take().map(Into::into))
            }
            [Segment::Field(tags)] if tags.as_str() == "tags" => {
                Ok(self.series.tags.take().map(|map| {
                    let iter = map.into_iter().map(|(k, v)| (k, v.into()));
                    remap::Value::from_iter(iter)
                }))
            }
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                Ok(self.delete_tag(field.as_str()).map(Into::into))
            }
            _ => Err(MetricPathError::InvalidPath {
                path: &path.to_string(),
                expected: VALID_METRIC_PATHS_SET,
            }
            .to_string()),
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
    for item in items.into_iter() {
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

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{offset::TimeZone, DateTime, Utc};
    use remap::{Path, Value};
    use shared::btreemap;
    use std::str::FromStr;

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

        counter.data.add(&delta.data);
        assert_eq!(
            counter,
            Metric::new(
                "counter",
                MetricKind::Incremental,
                MetricValue::Counter { value: 3.0 },
            )
        )
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

        gauge.data.add(&delta.data);
        assert_eq!(
            gauge,
            Metric::new(
                "gauge",
                MetricKind::Incremental,
                MetricValue::Gauge { value: -1.0 },
            )
        )
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

        set.data.add(&delta.data);
        assert_eq!(
            set,
            Metric::new(
                "set",
                MetricKind::Incremental,
                MetricValue::Set {
                    values: vec!["old".into(), "new".into()].into_iter().collect()
                },
            )
        )
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

        dist.data.add(&delta.data);
        assert_eq!(
            dist,
            Metric::new(
                "hist",
                MetricKind::Incremental,
                MetricValue::Distribution {
                    samples: samples![1.0 => 10, 1.0 => 20],
                    statistic: StatisticKind::Histogram
                },
            )
        )
    }

    #[test]
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
    fn object_metric_all_fields() {
        let metric = Metric::new(
            "zub",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_namespace(Some("zoob"))
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }))
        .with_timestamp(Some(Utc.ymd(2020, 12, 10).and_hms(12, 0, 0)));

        assert_eq!(
            Ok(Some(
                btreemap! {
                    "name" => "zub",
                    "namespace" => "zoob",
                    "timestamp" => Utc.ymd(2020, 12, 10).and_hms(12, 0, 0),
                    "tags" => btreemap! { "tig" => "tog" },
                    "kind" => "absolute",
                    "type" => "counter",
                }
                .into()
            )),
            metric.get(&Path::from_str(".").unwrap())
        );
    }

    #[test]
    fn object_metric_fields() {
        let mut metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .with_tags(Some({
            let mut map = MetricTags::new();
            map.insert("tig".to_string(), "tog".to_string());
            map
        }));

        let cases = vec![
            (
                "name",                    // Path
                Some(Value::from("name")), // Current value
                Value::from("namefoo"),    // New value
                false,                     // Test deletion
            ),
            ("namespace", None, "namespacefoo".into(), true),
            (
                "timestamp",
                None,
                Utc.ymd(2020, 12, 8).and_hms(12, 0, 0).into(),
                true,
            ),
            (
                "kind",
                Some(Value::from("absolute")),
                "incremental".into(),
                false,
            ),
            ("tags.thing", None, "footag".into(), true),
        ];

        for (path, current, new, delete) in cases {
            let path = Path::from_str(path).unwrap();

            assert_eq!(Ok(current), metric.get(&path));
            assert_eq!(Ok(()), metric.insert(&path, new.clone()));
            assert_eq!(Ok(Some(new.clone())), metric.get(&path));

            if delete {
                assert_eq!(Ok(Some(new)), metric.remove(&path, true));
                assert_eq!(Ok(None), metric.get(&path));
            }
        }
    }

    #[test]
    fn object_metric_invalid_paths() {
        let mut metric = Metric::new(
            "name",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        );

        let validpaths_get = vec![
            ".name",
            ".namespace",
            ".timestamp",
            ".kind",
            ".tags",
            ".type",
        ];

        let validpaths_set = vec![".name", ".namespace", ".timestamp", ".kind", ".tags"];

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_get.join(", ")
            )),
            metric.get(&Path::from_str("zork").unwrap())
        );

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            metric.insert(&Path::from_str("zork").unwrap(), "thing".into())
        );

        assert_eq!(
            Err(format!(
                "invalid path .zork: expected one of {}",
                validpaths_set.join(", ")
            )),
            metric.remove(&Path::from_str("zork").unwrap(), true)
        );

        assert_eq!(
            Err(format!(
                "invalid path .tags.foo.flork: expected one of {}",
                validpaths_get.join(", ")
            )),
            metric.get(&Path::from_str("tags.foo.flork").unwrap())
        );
    }
}

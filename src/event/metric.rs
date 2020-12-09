use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use remap::{object::Error as RemapError, Object, Path, Segment};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Metric {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
    pub kind: MetricKind,
    #[serde(flatten)]
    pub value: MetricValue,
}

#[derive(Debug, Hash, Clone, PartialEq, Deserialize, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
/// A metric may be an incremental value, updating the previous value of
/// the metric, or absolute, which sets the reference for future
/// increments.
pub enum MetricKind {
    Incremental,
    Absolute,
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
    /// A Distribution contains a set of sampled values paired with the
    /// rate at which they were observed.
    Distribution {
        values: Vec<f64>,
        sample_rates: Vec<u32>,
        statistic: StatisticKind,
    },
    /// An AggregatedHistogram contains a set of observations which are
    /// counted into buckets. The value of the bucket is the upper bound
    /// on the range of values within the bucket. The lower bound on the
    /// range is just higher than the previous bucket, or zero for the
    /// first bucket. It also contains the total count of all
    /// observations and their sum to allow calculating the mean.
    AggregatedHistogram {
        buckets: Vec<f64>,
        counts: Vec<u32>,
        count: u32,
        sum: f64,
    },
    /// An AggregatedSummary contains a set of observations which are
    /// counted into a number of quantiles. Each quantile contains the
    /// upper value of the quantile (0 <= φ <= 1). It also contains the
    /// total count of all observations and their sum to allow
    /// calculating the mean.
    AggregatedSummary {
        quantiles: Vec<f64>,
        values: Vec<f64>,
        count: u32,
        sum: f64,
    },
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
    /// Create a new Metric from this with all the data but marked as absolute.
    pub fn to_absolute(&self) -> Self {
        Self {
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            timestamp: self.timestamp,
            tags: self.tags.clone(),
            kind: MetricKind::Absolute,
            value: self.value.clone(),
        }
    }

    /// Add the data from the other metric to this one. The `other` must
    /// be relative and contain the same value type as this one.
    pub fn add(&mut self, other: &Self) {
        if other.kind.is_absolute() {
            return;
        }

        match (&mut self.value, &other.value) {
            (MetricValue::Counter { ref mut value }, MetricValue::Counter { value: value2 }) => {
                *value += value2;
            }
            (MetricValue::Gauge { ref mut value }, MetricValue::Gauge { value: value2 }) => {
                *value += value2;
            }
            (MetricValue::Set { ref mut values }, MetricValue::Set { values: values2 }) => {
                values.extend(values2.iter().map(Into::into));
            }
            (
                MetricValue::Distribution {
                    ref mut values,
                    ref mut sample_rates,
                    statistic: statistic_a,
                },
                MetricValue::Distribution {
                    values: values2,
                    sample_rates: sample_rates2,
                    statistic: statistic_b,
                },
            ) if statistic_a == statistic_b => {
                values.extend_from_slice(&values2);
                sample_rates.extend_from_slice(&sample_rates2);
            }
            (
                MetricValue::AggregatedHistogram {
                    ref buckets,
                    ref mut counts,
                    ref mut count,
                    ref mut sum,
                },
                MetricValue::AggregatedHistogram {
                    buckets: buckets2,
                    counts: counts2,
                    count: count2,
                    sum: sum2,
                },
            ) => {
                if buckets == buckets2 && counts.len() == counts2.len() {
                    for (i, c) in counts2.iter().enumerate() {
                        counts[i] += c;
                    }
                    *count += count2;
                    *sum += sum2;
                }
            }
            _ => {}
        }
    }

    /// Set all the values of this metric to zero without emptying
    /// it. This keeps all the bucket/value vectors for the histogram
    /// and summary metric types intact while zeroing the
    /// counts. Distribution metrics are emptied of all their values.
    pub fn reset(&mut self) {
        match &mut self.value {
            MetricValue::Counter { ref mut value } => {
                *value = 0.0;
            }
            MetricValue::Gauge { ref mut value } => {
                *value = 0.0;
            }
            MetricValue::Set { ref mut values } => {
                values.clear();
            }
            MetricValue::Distribution {
                ref mut values,
                ref mut sample_rates,
                ..
            } => {
                values.clear();
                sample_rates.clear();
            }
            MetricValue::AggregatedHistogram {
                ref mut counts,
                ref mut count,
                ref mut sum,
                ..
            } => {
                for c in counts.iter_mut() {
                    *c = 0;
                }
                *count = 0;
                *sum = 0.0;
            }
            MetricValue::AggregatedSummary {
                ref mut values,
                ref mut count,
                ref mut sum,
                ..
            } => {
                for v in values.iter_mut() {
                    *v = 0.0;
                }
                *count = 0;
                *sum = 0.0;
            }
        }
    }

    /// Convert the metrics_runtime::Measurement value plus the name and
    /// labels from a Key into our internal Metric format.
    pub fn from_metric_kv(key: metrics::Key, handle: metrics_util::Handle) -> Self {
        let value = match handle {
            metrics_util::Handle::Counter(_) => MetricValue::Counter {
                value: handle.read_counter() as f64,
            },
            metrics_util::Handle::Gauge(_) => MetricValue::Gauge {
                value: handle.read_gauge() as f64,
            },
            metrics_util::Handle::Histogram(_) => {
                let values = handle.read_histogram();
                let values = values.into_iter().map(|i| i as f64).collect::<Vec<_>>();
                // Each sample in the source measurement has an
                // effective sample rate of 1, so create an array of
                // such of the same length as the values.
                let sample_rates = vec![1; values.len()];
                MetricValue::Distribution {
                    values,
                    sample_rates,
                    statistic: StatisticKind::Histogram,
                }
            }
        };

        let labels = key
            .labels()
            .map(|label| (String::from(label.key()), String::from(label.value())))
            .collect::<BTreeMap<_, _>>();

        Self {
            name: key.name().to_string(),
            namespace: Some("vector".to_string()),
            timestamp: Some(Utc::now()),
            tags: if labels.is_empty() {
                None
            } else {
                Some(labels)
            },
            kind: MetricKind::Absolute,
            value,
        }
    }

    /// Returns `true` if `name` tag is present, and matches the provided `value`
    pub fn tag_matches(&self, name: &str, value: &str) -> bool {
        self.tags
            .as_ref()
            .filter(|t| t.get(name).filter(|v| *v == value).is_some())
            .is_some()
    }

    /// Returns the string value of a tag, if it exists
    pub fn tag_value(&self, name: &str) -> Option<String> {
        self.tags.as_ref().and_then(|t| t.get(name).cloned())
    }

    /// Sets or updates the string value of a tag
    pub fn set_tag_value(&mut self, name: String, value: String) {
        self.tags
            .get_or_insert_with(BTreeMap::new)
            .insert(name, value);
    }

    /// Deletes the tag, if it exists.
    pub fn delete_tag(&mut self, name: &str) {
        self.tags.as_mut().and_then(|tags| tags.remove(name));
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
        if let Some(timestamp) = &self.timestamp {
            write!(fmt, "{:?} ", timestamp)?;
        }
        if let Some(namespace) = &self.namespace {
            write_word(fmt, namespace)?;
            write!(fmt, "_")?;
        }
        write_word(fmt, &self.name)?;
        write!(fmt, "{{")?;
        if let Some(tags) = &self.tags {
            write_list(fmt, ",", tags.iter(), |fmt, (tag, value)| {
                write_word(fmt, tag).and_then(|()| write!(fmt, "={:?}", value))
            })?;
        }
        write!(
            fmt,
            "}} {} ",
            match self.kind {
                MetricKind::Absolute => '=',
                MetricKind::Incremental => '+',
            }
        )?;
        match &self.value {
            MetricValue::Counter { value } => write!(fmt, "{}", value),
            MetricValue::Gauge { value } => write!(fmt, "{}", value),
            MetricValue::Set { values } => {
                write_list(fmt, " ", values.iter(), |fmt, value| write_word(fmt, value))
            }
            MetricValue::Distribution {
                values,
                sample_rates,
                statistic,
            } => {
                write!(
                    fmt,
                    "{} ",
                    match statistic {
                        StatisticKind::Histogram => "histogram",
                        StatisticKind::Summary => "summary",
                    }
                )?;
                write_list(
                    fmt,
                    " ",
                    values.iter().zip(sample_rates.iter()),
                    |fmt, (value, rate)| write!(fmt, "{}@{}", rate, value),
                )
            }
            MetricValue::AggregatedHistogram {
                buckets,
                counts,
                count,
                sum,
            } => {
                write!(fmt, "count={} sum={} ", count, sum)?;
                write_list(
                    fmt,
                    " ",
                    buckets.iter().zip(counts.iter()),
                    |fmt, (bucket, count)| write!(fmt, "{}@{}", count, bucket),
                )
            }
            MetricValue::AggregatedSummary {
                quantiles,
                values,
                count,
                sum,
            } => {
                write!(fmt, "count={} sum={} ", count, sum)?;
                write_list(
                    fmt,
                    " ",
                    quantiles.iter().zip(values.iter()),
                    |fmt, (quantile, value)| write!(fmt, "{}@{}", quantile, value),
                )
            }
        }
    }
}

impl Object for Metric {
    fn insert(&mut self, path: &remap::Path, value: remap::Value) -> Result<(), RemapError> {
        if path.is_root() {
            return Err(RemapError::SetRoot);
        }

        match path.segments() {
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                let value = value
                    .try_bytes()
                    .map_err(|e| RemapError::InvalidType(e.to_string()))?;
                self.set_tag_value(
                    field.as_str().to_owned(),
                    String::from_utf8_lossy(&value).into_owned(),
                );
                Ok(())
            }
            [Segment::Field(name)] if name.as_str() == "name" => {
                let value = value
                    .try_bytes()
                    .map_err(|e| RemapError::InvalidType(e.to_string()))?;
                self.name = String::from_utf8_lossy(&value).into_owned();
                Ok(())
            }
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                let value = value
                    .try_bytes()
                    .map_err(|e| RemapError::InvalidType(e.to_string()))?;
                self.namespace = Some(String::from_utf8_lossy(&value).into_owned());
                Ok(())
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                let value = value
                    .try_timestamp()
                    .map_err(|e| RemapError::InvalidType(e.to_string()))?;
                self.timestamp = Some(value);
                Ok(())
            }
            [Segment::Field(kind)] if kind.as_str() == "kind" => {
                let value = value
                    .try_bytes()
                    .map_err(|e| RemapError::InvalidType(e.to_string()))?;
                match String::from_utf8_lossy(&value).as_ref() {
                    "absolute" => {
                        self.kind = MetricKind::Absolute;
                        Ok(())
                    }
                    "incremental" => {
                        self.kind = MetricKind::Incremental;
                        Ok(())
                    }
                    _ => Err(RemapError::Other(
                        "metric kind must be `absolute` or `incremental`".into(),
                    )),
                }
            }
            _ => Err(RemapError::InvalidPath(
                format!("{}", path),
                remap::object::ValidPaths(vec![
                    ".name",
                    ".namespace",
                    ".timestamp",
                    ".kind",
                    ".tags",
                ]),
            )),
        }
    }

    fn get(&self, path: &remap::Path) -> Result<Option<remap::Value>, RemapError> {
        // TODO construct a map of the whole metric.
        if path.is_root() {
            return Err(RemapError::SetRoot);
        }

        match path.segments() {
            [Segment::Field(name)] if name.as_str() == "name" => Ok(Some(self.name.clone().into())),
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                Ok(self.namespace.clone().map(Into::into))
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                Ok(self.timestamp.map(Into::into))
            }
            [Segment::Field(kind)] if kind.as_str() == "kind" => Ok(Some(
                match self.kind {
                    MetricKind::Absolute => "absolute",
                    MetricKind::Incremental => "incremental",
                }
                .into(),
            )),
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                Ok(self.tag_value(field.as_str()).map(|value| value.into()))
            }
            _ => Err(RemapError::InvalidPath(
                format!("{}", path),
                remap::object::ValidPaths(vec![
                    ".name",
                    ".namespace",
                    ".timestamp",
                    ".kind",
                    ".tags",
                ]),
            )),
        }
    }

    fn paths(&self) -> Result<Vec<remap::Path>, RemapError> {
        Ok(["name", "namespace", "timestamp", "tags"]
            .iter()
            .map(|path| Path::from_str(path).expect("invalid path"))
            .collect())
    }

    fn remove(&mut self, path: &remap::Path, _compact: bool) -> Result<(), RemapError> {
        if path.is_root() {
            return Err(RemapError::SetRoot);
        }

        match path.segments() {
            [Segment::Field(namespace)] if namespace.as_str() == "namespace" => {
                self.namespace = None;
                Ok(())
            }
            [Segment::Field(timestamp)] if timestamp.as_str() == "timestamp" => {
                self.timestamp = None;
                Ok(())
            }
            [Segment::Field(tags), Segment::Field(field)] if tags.as_str() == "tags" => {
                self.delete_tag(field.as_str());
                Ok(())
            }
            _ => Err(RemapError::InvalidPath(
                format!("{}", path),
                remap::object::ValidPaths(vec![
                    ".name",
                    ".namespace",
                    ".timestamp",
                    ".kind",
                    ".tags",
                ]),
            )),
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
    I: Iterator<Item = T>,
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

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{offset::TimeZone, DateTime, Utc};
    use remap::{Path, Value};

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> BTreeMap<String, String> {
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
        let mut counter = Metric {
            name: "counter".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 1.0 },
        };

        let delta = Metric {
            name: "counter".into(),
            namespace: Some("vector".to_string()),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 2.0 },
        };

        counter.add(&delta);
        assert_eq!(
            counter,
            Metric {
                name: "counter".into(),
                namespace: None,
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 3.0 },
            }
        )
    }

    #[test]
    fn merge_gauges() {
        let mut gauge = Metric {
            name: "gauge".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: 1.0 },
        };

        let delta = Metric {
            name: "gauge".into(),
            namespace: Some("vector".to_string()),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: -2.0 },
        };

        gauge.add(&delta);
        assert_eq!(
            gauge,
            Metric {
                name: "gauge".into(),
                namespace: None,
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: -1.0 },
            }
        )
    }

    #[test]
    fn merge_sets() {
        let mut set = Metric {
            name: "set".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["old".into()].into_iter().collect(),
            },
        };

        let delta = Metric {
            name: "set".into(),
            namespace: Some("vector".to_string()),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["new".into()].into_iter().collect(),
            },
        };

        set.add(&delta);
        assert_eq!(
            set,
            Metric {
                name: "set".into(),
                namespace: None,
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Set {
                    values: vec!["old".into(), "new".into()].into_iter().collect()
                },
            }
        )
    }

    #[test]
    fn merge_histograms() {
        let mut dist = Metric {
            name: "hist".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0],
                sample_rates: vec![10],
                statistic: StatisticKind::Histogram,
            },
        };

        let delta = Metric {
            name: "hist".into(),
            namespace: Some("vector".to_string()),
            timestamp: Some(ts()),
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.0],
                sample_rates: vec![20],
                statistic: StatisticKind::Histogram,
            },
        };

        dist.add(&delta);
        assert_eq!(
            dist,
            Metric {
                name: "hist".into(),
                namespace: None,
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![1.0, 1.0],
                    sample_rates: vec![10, 20],
                    statistic: StatisticKind::Histogram
                },
            }
        )
    }

    #[test]
    fn display() {
        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "one".into(),
                    namespace: None,
                    timestamp: None,
                    tags: Some(tags()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 1.23 },
                }
            ),
            r#"one{empty_tag="",normal_tag="value",true_tag="true"} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "two word".into(),
                    namespace: None,
                    timestamp: Some(ts()),
                    tags: None,
                    kind: MetricKind::Incremental,
                    value: MetricValue::Gauge { value: 2.0 }
                }
            ),
            r#"2018-11-14T08:09:10.000000011Z "two word"{} + 2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "namespace".into(),
                    namespace: Some("vector".to_string()),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 1.23 },
                }
            ),
            r#"vector_namespace{} = 1.23"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "namespace".into(),
                    namespace: Some("vector host".to_string()),
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 1.23 },
                }
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
                Metric {
                    name: "three".into(),
                    namespace: None,
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Set { values }
                }
            ),
            r#"three{} = "four=4" "thrəë" v1 v2_two"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "four".into(),
                    namespace: None,
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::Distribution {
                        values: vec![1.0, 2.0],
                        sample_rates: vec![3, 4],
                        statistic: StatisticKind::Histogram,
                    }
                }
            ),
            r#"four{} = histogram 3@1 4@2"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "five".into(),
                    namespace: None,
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedHistogram {
                        buckets: vec![51.0, 52.0],
                        counts: vec![53, 54],
                        count: 107,
                        sum: 103.0,
                    }
                }
            ),
            r#"five{} = count=107 sum=103 53@51 54@52"#
        );

        assert_eq!(
            format!(
                "{}",
                Metric {
                    name: "six".into(),
                    namespace: None,
                    timestamp: None,
                    tags: None,
                    kind: MetricKind::Absolute,
                    value: MetricValue::AggregatedSummary {
                        quantiles: vec![1.0, 2.0],
                        values: vec![63.0, 64.0],
                        count: 2,
                        sum: 127.0,
                    }
                }
            ),
            r#"six{} = count=2 sum=127 1@63 2@64"#
        );
    }

    #[test]
    fn object_metric_fields() {
        let mut metric = Metric {
            name: "name".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.23 },
        };

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
            assert_eq!(Ok(Some(new)), metric.get(&path));

            if delete {
                assert_eq!(Ok(()), metric.remove(&path, true));
                assert_eq!(Ok(None), metric.get(&path));
            }
        }
    }

    #[test]
    fn object_metric_invalid_paths() {
        let mut metric = Metric {
            name: "name".into(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.23 },
        };

        let validpaths =
            remap::object::ValidPaths(vec![".name", ".namespace", ".timestamp", ".kind", ".tags"]);

        assert_eq!(
            Err(RemapError::InvalidPath(".zork".into(), validpaths.clone())),
            metric.get(&Path::from_str("zork").unwrap())
        );

        assert_eq!(
            Err(RemapError::InvalidPath(".zork".into(), validpaths.clone())),
            metric.insert(&Path::from_str("zork").unwrap(), "thing".into())
        );

        assert_eq!(
            Err(RemapError::InvalidPath(".zork".into(), validpaths.clone())),
            metric.remove(&Path::from_str("zork").unwrap(), true)
        );

        assert_eq!(
            Err(RemapError::InvalidPath(
                ".tags.foo.flork".into(),
                validpaths
            )),
            metric.get(&Path::from_str("tags.foo.flork").unwrap())
        );
    }
}

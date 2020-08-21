use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use metrics_core::Key;
use metrics_runtime::Measurement;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Metric {
    pub name: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub tags: Option<BTreeMap<String, String>>,
    pub kind: MetricKind,
    #[serde(flatten)]
    pub value: MetricValue,
}

#[derive(Debug, Hash, Clone, PartialEq, Deserialize, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    Incremental,
    Absolute,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, is_enum_variant)]
#[serde(rename_all = "snake_case")]
pub enum MetricValue {
    Counter {
        value: f64,
    },
    Gauge {
        value: f64,
    },
    Set {
        values: BTreeSet<String>,
    },
    Distribution {
        values: Vec<f64>,
        sample_rates: Vec<u32>,
        statistic: StatisticKind,
    },
    AggregatedHistogram {
        buckets: Vec<f64>,
        counts: Vec<u32>,
        count: u32,
        sum: f64,
    },
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
    pub fn to_absolute(&self) -> Self {
        Self {
            name: self.name.clone(),
            timestamp: self.timestamp,
            tags: self.tags.clone(),
            kind: MetricKind::Absolute,
            value: self.value.clone(),
        }
    }

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
                for val in values2 {
                    values.insert(val.to_string());
                }
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

    pub fn from_measurement(key: Key, measurement: Measurement) -> Self {
        let value = match measurement {
            Measurement::Counter(v) => MetricValue::Counter { value: v as f64 },
            Measurement::Gauge(v) => MetricValue::Gauge { value: v as f64 },
            Measurement::Histogram(packed) => {
                let values = packed
                    .decompress()
                    .into_iter()
                    .map(|i| i as f64)
                    .collect::<Vec<_>>();
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
}

impl Display for Metric {
    /// Display a metric using something like Prometheus' text format:
    ///
    /// TIMESTAMP NAME{TAGS} KIND DATA
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
    /// 2020-08-12T20:23:37.248661343Z bytes_processed{component_kind="sink",component_type="blackhole"} = 6391
    /// ```
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        if let Some(timestamp) = &self.timestamp {
            write!(fmt, "{:?} ", timestamp)?;
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
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 1.0 },
        };

        let delta = Metric {
            name: "counter".into(),
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
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: 1.0 },
        };

        let delta = Metric {
            name: "gauge".into(),
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
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["old".into()].into_iter().collect(),
            },
        };

        let delta = Metric {
            name: "set".into(),
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
                    timestamp: Some(ts()),
                    tags: None,
                    kind: MetricKind::Incremental,
                    value: MetricValue::Gauge { value: 2.0 }
                }
            ),
            r#"2018-11-14T08:09:10.000000011Z "two word"{} + 2"#
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
}

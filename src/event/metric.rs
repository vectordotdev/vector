use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

impl Metric {
    pub fn into_absolute(&self) -> Self {
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
                },
                MetricValue::Distribution {
                    values: values2,
                    sample_rates: sample_rates2,
                },
            ) => {
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
                },
            }
        )
    }
}

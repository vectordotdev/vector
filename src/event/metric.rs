use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, is_enum_variant)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Metric {
    Counter {
        name: String,
        val: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    Histogram {
        name: String,
        val: f64,
        sample_rate: u32,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    Gauge {
        name: String,
        val: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    Set {
        name: String,
        val: String,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedCounter {
        name: String,
        val: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedGauge {
        name: String,
        val: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedSet {
        name: String,
        values: HashSet<String>,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedDistribution {
        name: String,
        values: Vec<f64>,
        sample_rates: Vec<u32>,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedHistogram {
        name: String,
        buckets: Vec<f64>,
        counts: Vec<u32>,
        count: u32,
        sum: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedSummary {
        name: String,
        quantiles: Vec<f64>,
        values: Vec<f64>,
        count: u32,
        sum: f64,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
}

impl Metric {
    pub fn tags(&self) -> &Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
            Metric::AggregatedCounter { tags, .. } => tags,
            Metric::AggregatedGauge { tags, .. } => tags,
            Metric::AggregatedSet { tags, .. } => tags,
            Metric::AggregatedDistribution { tags, .. } => tags,
            Metric::AggregatedHistogram { tags, .. } => tags,
            Metric::AggregatedSummary { tags, .. } => tags,
        }
    }

    pub fn tags_mut(&mut self) -> &mut Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
            Metric::AggregatedCounter { tags, .. } => tags,
            Metric::AggregatedGauge { tags, .. } => tags,
            Metric::AggregatedSet { tags, .. } => tags,
            Metric::AggregatedDistribution { tags, .. } => tags,
            Metric::AggregatedHistogram { tags, .. } => tags,
            Metric::AggregatedSummary { tags, .. } => tags,
        }
    }

    pub fn is_aggregated(&self) -> bool {
        match self {
            Metric::Counter { .. } => false,
            Metric::Gauge { .. } => false,
            Metric::Histogram { .. } => false,
            Metric::Set { .. } => false,
            Metric::AggregatedCounter { .. } => true,
            Metric::AggregatedGauge { .. } => true,
            Metric::AggregatedSet { .. } => true,
            Metric::AggregatedDistribution { .. } => true,
            Metric::AggregatedHistogram { .. } => true,
            Metric::AggregatedSummary { .. } => true,
        }
    }

    pub fn into_aggregated(self) -> Metric {
        match self {
            Metric::Counter {
                name,
                val,
                timestamp,
                tags,
            } => Metric::AggregatedCounter {
                name,
                val,
                timestamp,
                tags,
            },
            Metric::Gauge {
                name,
                val,
                timestamp,
                tags,
            } => Metric::AggregatedGauge {
                name,
                val,
                timestamp,
                tags,
            },
            Metric::Histogram {
                name,
                val,
                sample_rate,
                timestamp,
                tags,
            } => Metric::AggregatedDistribution {
                name,
                values: vec![val],
                sample_rates: vec![sample_rate],
                timestamp,
                tags,
            },
            Metric::Set {
                name,
                val,
                timestamp,
                tags,
            } => Metric::AggregatedSet {
                name,
                values: vec![val].into_iter().collect(),
                timestamp,
                tags,
            },
            m @ Metric::AggregatedCounter { .. } => m,
            m @ Metric::AggregatedGauge { .. } => m,
            m @ Metric::AggregatedSet { .. } => m,
            m @ Metric::AggregatedDistribution { .. } => m,
            m @ Metric::AggregatedHistogram { .. } => m,
            m @ Metric::AggregatedSummary { .. } => m,
        }
    }

    pub fn add(&mut self, other: &Self) {
        match (self, other) {
            (Metric::AggregatedCounter { ref mut val, .. }, Metric::Counter { val: inc, .. }) => {
                *val += *inc;
            }
            (Metric::AggregatedGauge { ref mut val, .. }, Metric::Gauge { val: inc, .. }) => {
                *val += *inc;
            }
            (Metric::AggregatedSet { ref mut values, .. }, Metric::Set { val, .. }) => {
                values.insert(val.to_owned());
            }
            (
                Metric::AggregatedDistribution {
                    ref mut values,
                    ref mut sample_rates,
                    ..
                },
                Metric::Histogram {
                    val, sample_rate, ..
                },
            ) => {
                values.push(*val);
                sample_rates.push(*sample_rate);
            }
            _ => {}
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

    fn tags() -> HashMap<String, String> {
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
        let mut counter = Metric::AggregatedCounter {
            name: "counter".into(),
            val: 1.0,
            timestamp: None,
            tags: None,
        };

        let delta = Metric::Counter {
            name: "counter".into(),
            val: 2.0,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        counter.add(&delta);
        assert_eq!(
            counter,
            Metric::AggregatedCounter {
                name: "counter".into(),
                val: 3.0,
                timestamp: None,
                tags: None,
            }
        )
    }

    #[test]
    fn merge_gauges() {
        let mut gauge = Metric::AggregatedGauge {
            name: "gauge".into(),
            val: 1.0,
            timestamp: None,
            tags: None,
        };

        let delta = Metric::Gauge {
            name: "gauge".into(),
            val: -2.0,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        gauge.add(&delta);
        assert_eq!(
            gauge,
            Metric::AggregatedGauge {
                name: "gauge".into(),
                val: -1.0,
                timestamp: None,
                tags: None,
            }
        )
    }

    #[test]
    fn merge_sets() {
        let mut set = Metric::AggregatedSet {
            name: "set".into(),
            values: vec!["old".into()].into_iter().collect(),
            timestamp: None,
            tags: None,
        };

        let delta = Metric::Set {
            name: "set".into(),
            val: "new".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        set.add(&delta);
        assert_eq!(
            set,
            Metric::AggregatedSet {
                name: "set".into(),
                values: vec!["old".into(), "new".into()].into_iter().collect(),
                timestamp: None,
                tags: None,
            }
        )
    }

    #[test]
    fn merge_histograms() {
        let mut dist = Metric::AggregatedDistribution {
            name: "hist".into(),
            values: vec![1.0],
            sample_rates: vec![10],
            timestamp: None,
            tags: None,
        };

        let delta = Metric::Histogram {
            name: "hist".into(),
            val: 1.0,
            sample_rate: 20,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        dist.add(&delta);
        assert_eq!(
            dist,
            Metric::AggregatedDistribution {
                name: "hist".into(),
                values: vec![1.0, 1.0],
                sample_rates: vec![10, 20],
                timestamp: None,
                tags: None,
            }
        )
    }
}

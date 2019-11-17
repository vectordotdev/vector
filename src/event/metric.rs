use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metric {
    pub name: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub tags: Option<HashMap<String, String>>,
    pub value: MetricValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, is_enum_variant)]
#[serde(tag = "value", rename_all = "snake_case")]
pub enum MetricValue {
    Counter {
        val: f64,
    },
    Histogram {
        val: f64,
        sample_rate: u32,
    },
    Gauge {
        val: f64,
    },
    Set {
        val: String,
    },
    AggregatedCounter {
        val: f64,
    },
    AggregatedGauge {
        val: f64,
    },
    AggregatedSet {
        values: HashSet<String>,
    },
    AggregatedDistribution {
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

impl MetricValue {
    pub fn is_aggregated(&self) -> bool {
        match self {
            MetricValue::Counter { .. } => false,
            MetricValue::Gauge { .. } => false,
            MetricValue::Histogram { .. } => false,
            MetricValue::Set { .. } => false,
            MetricValue::AggregatedCounter { .. } => true,
            MetricValue::AggregatedGauge { .. } => true,
            MetricValue::AggregatedSet { .. } => true,
            MetricValue::AggregatedDistribution { .. } => true,
            MetricValue::AggregatedHistogram { .. } => true,
            MetricValue::AggregatedSummary { .. } => true,
        }
    }
}

impl Metric {
    pub fn into_aggregated(self) -> Metric {
        let value = match self.value {
            MetricValue::Counter { val } => MetricValue::AggregatedCounter { val },
            MetricValue::Gauge { val } => MetricValue::AggregatedGauge { val },
            MetricValue::Histogram { val, sample_rate } => MetricValue::AggregatedDistribution {
                values: vec![val],
                sample_rates: vec![sample_rate],
            },
            MetricValue::Set { val } => MetricValue::AggregatedSet {
                values: vec![val].into_iter().collect(),
            },
            m @ MetricValue::AggregatedCounter { .. } => m,
            m @ MetricValue::AggregatedGauge { .. } => m,
            m @ MetricValue::AggregatedSet { .. } => m,
            m @ MetricValue::AggregatedDistribution { .. } => m,
            m @ MetricValue::AggregatedHistogram { .. } => m,
            m @ MetricValue::AggregatedSummary { .. } => m,
        };

        Metric {
            name: self.name,
            timestamp: self.timestamp,
            tags: self.tags,
            value,
        }
    }

    pub fn add(&mut self, other: &Self) {
        match (&mut self.value, &other.value) {
            (MetricValue::Counter { ref mut val, .. }, MetricValue::Counter { val: inc, .. }) => {
                *val += inc;
            }
            (
                MetricValue::AggregatedCounter { ref mut val, .. },
                MetricValue::Counter { val: inc, .. },
            ) => {
                *val += inc;
            }
            (
                MetricValue::AggregatedGauge { ref mut val, .. },
                MetricValue::Gauge { val: inc, .. },
            ) => {
                *val += inc;
            }
            (
                MetricValue::AggregatedSet { ref mut values, .. },
                MetricValue::Set { ref val, .. },
            ) => {
                values.insert(val.to_owned());
            }
            (
                MetricValue::AggregatedDistribution {
                    ref mut values,
                    ref mut sample_rates,
                    ..
                },
                MetricValue::Histogram {
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

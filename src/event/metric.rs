use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::Serialize;
use std::collections::HashMap;

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
        direction: Option<Direction>,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    Set {
        name: String,
        val: String,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
    AggregatedHistogram {
        name: String,
        buckets: Vec<f64>,
        counts: Vec<u32>,
        count: u32,
        sum: f64,
        stats: Option<Stats>,
        timestamp: Option<DateTime<Utc>>,
        tags: Option<HashMap<String, String>>,
    },
}

// https://github.com/influxdata/telegraf/tree/master/plugins/parsers/dropwizard
// https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/publishingMetrics.html#publishingDataPoints1
// https://docs.datadoghq.com/developers/faq/data-aggregation-with-dogstatsd-threadstats/#aggregation-rules-per-metric-type
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Stats {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Direction {
    Plus,
    Minus,
}

impl Metric {
    pub fn tags(&self) -> &Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::AggregatedHistogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
        }
    }

    pub fn tags_mut(&mut self) -> &mut Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::AggregatedHistogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
        }
    }

    pub fn merge(&mut self, other: &Self) {
        match (self, other) {
            (
                Metric::Counter {
                    ref mut name,
                    ref mut val,
                    ref mut timestamp,
                    ref mut tags,
                },
                Metric::Counter {
                    name: new_name,
                    val: new_val,
                    timestamp: new_timestamp,
                    tags: new_tags,
                },
            ) => {
                if name == new_name {
                    *val += *new_val;
                    *timestamp = *new_timestamp;
                    *tags = new_tags.clone();
                }
            }
            (
                Metric::Gauge {
                    ref mut name,
                    ref mut val,
                    direction: None,
                    ref mut timestamp,
                    ref mut tags,
                },
                Metric::Gauge {
                    name: new_name,
                    val: new_val,
                    timestamp: new_timestamp,
                    direction: new_direction,
                    tags: new_tags,
                },
            ) => {
                if name == new_name {
                    if new_direction.is_none() {
                        *val = *new_val;
                    } else {
                        let delta = match new_direction {
                            None => 0.0,
                            Some(Direction::Plus) => *val,
                            Some(Direction::Minus) => -*val,
                        };
                        *val += delta;
                    };
                    *timestamp = *new_timestamp;
                    *tags = new_tags.clone();
                }
            }
            (
                Metric::Set {
                    ref mut name,
                    ref mut val,
                    ref mut timestamp,
                    ref mut tags,
                },
                Metric::Set {
                    name: new_name,
                    val: new_val,
                    timestamp: new_timestamp,
                    tags: new_tags,
                },
            ) => {
                if name == new_name {
                    *val = new_val.clone();
                    *timestamp = *new_timestamp;
                    *tags = new_tags.clone();
                }
            }
            (
                Metric::Histogram {
                    ref mut name,
                    ref mut val,
                    ref mut sample_rate,
                    ref mut timestamp,
                    ref mut tags,
                },
                Metric::Histogram {
                    name: new_name,
                    val: new_val,
                    sample_rate: new_sample_rate,
                    timestamp: new_timestamp,
                    tags: new_tags,
                },
            ) => {
                if name == new_name && val == new_val {
                    *sample_rate += *new_sample_rate;
                    *timestamp = *new_timestamp;
                    *tags = new_tags.clone();
                };
            }
            (
                Metric::AggregatedHistogram {
                    ref mut name,
                    ref mut buckets,
                    ref mut counts,
                    ref mut count,
                    ref mut sum,
                    ref mut stats,
                    ref mut timestamp,
                    ref mut tags,
                },
                Metric::AggregatedHistogram {
                    name: new_name,
                    buckets: new_buckets,
                    counts: new_counts,
                    count: new_count,
                    sum: new_sum,
                    stats: new_stats,
                    timestamp: new_timestamp,
                    tags: new_tags,
                },
            ) => {
                if name == new_name && buckets == new_buckets {
                    for i in 0..counts.len() {
                        counts[i] += new_counts[i];
                    }
                    *sum += new_sum;
                    *count += new_count;
                    match (&stats, &new_stats) {
                        (None, Some(_)) => {
                            *stats = new_stats.clone();
                        }
                        (Some(s1), Some(s2)) => {
                            *stats = Some(Stats {
                                min: s1.min.min(s2.min),
                                max: s1.max.max(s2.max),
                            });
                        }
                        _ => {}
                    };
                    *timestamp = *new_timestamp;
                    *tags = new_tags.clone();
                }
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
        let mut counter1 = Metric::Counter {
            name: "counter".into(),
            val: 1.0,
            timestamp: None,
            tags: None,
        };

        let counter2 = Metric::Counter {
            name: "counter".into(),
            val: 2.0,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        counter1.merge(&counter2);
        assert_eq!(
            counter1,
            Metric::Counter {
                name: "counter".into(),
                val: 3.0,
                timestamp: Some(ts()),
                tags: Some(tags()),
            }
        )
    }

    #[test]
    fn merge_incompatible_counters() {
        let mut counter1 = Metric::Counter {
            name: "first".into(),
            val: 1.0,
            timestamp: None,
            tags: None,
        };

        let counter2 = Metric::Counter {
            name: "second".into(),
            val: 2.0,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        counter1.merge(&counter2);
        assert_eq!(
            counter1,
            Metric::Counter {
                name: "first".into(),
                val: 1.0,
                timestamp: None,
                tags: None,
            }
        )
    }

    #[test]
    fn merge_gauges() {
        let mut gauge1 = Metric::Gauge {
            name: "gauge".into(),
            val: 1.0,
            direction: None,
            timestamp: None,
            tags: None,
        };

        let gauge2 = Metric::Gauge {
            name: "gauge".into(),
            val: 2.0,
            direction: None,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        gauge1.merge(&gauge2);
        assert_eq!(
            gauge1,
            Metric::Gauge {
                name: "gauge".into(),
                val: 2.0,
                direction: None,
                timestamp: Some(ts()),
                tags: Some(tags()),
            }
        )
    }

    #[test]
    fn merge_sets() {
        let mut set1 = Metric::Set {
            name: "set".into(),
            val: "old".into(),
            timestamp: None,
            tags: None,
        };

        let set2 = Metric::Set {
            name: "set".into(),
            val: "new".into(),
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        set1.merge(&set2);
        assert_eq!(
            set1,
            Metric::Set {
                name: "set".into(),
                val: "new".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
            }
        )
    }

    #[test]
    fn merge_histograms() {
        let mut hist1 = Metric::Histogram {
            name: "hist".into(),
            val: 1.0,
            sample_rate: 10,
            timestamp: None,
            tags: None,
        };

        let hist2 = Metric::Histogram {
            name: "hist".into(),
            val: 1.0,
            sample_rate: 20,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        hist1.merge(&hist2);
        assert_eq!(
            hist1,
            Metric::Histogram {
                name: "hist".into(),
                val: 1.0,
                sample_rate: 30,
                timestamp: Some(ts()),
                tags: Some(tags()),
            }
        )
    }

    #[test]
    fn merge_aggregated_histograms() {
        let mut hist1 = Metric::AggregatedHistogram {
            name: "hist".into(),
            buckets: vec![1.0, 2.0, 4.0],
            counts: vec![1, 5, 15],
            count: 21,
            sum: 10.0,
            stats: None,
            timestamp: None,
            tags: None,
        };

        let hist2 = Metric::AggregatedHistogram {
            name: "hist".into(),
            buckets: vec![1.0, 2.0, 4.0],
            counts: vec![2, 10, 30],
            count: 42,
            sum: 20.0,
            stats: None,
            timestamp: Some(ts()),
            tags: Some(tags()),
        };

        hist1.merge(&hist2);
        assert_eq!(
            hist1,
            Metric::AggregatedHistogram {
                name: "hist".into(),
                buckets: vec![1.0, 2.0, 4.0],
                counts: vec![3, 15, 45],
                count: 63,
                sum: 30.0,
                stats: None,
                timestamp: Some(ts()),
                tags: Some(tags()),
            }
        )
    }
}

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Direction {
    Plus,
    Minus,
}

impl Metric {
    pub fn name(&self) -> &str {
        match self {
            Metric::Counter { name, .. } => name,
            Metric::Gauge { name, .. } => name,
            Metric::Histogram { name, .. } => name,
            Metric::Set { name, .. } => name,
        }
    }

    pub fn tags(&self) -> &Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
        }
    }

    pub fn tags_mut(&mut self) -> &mut Option<HashMap<String, String>> {
        match self {
            Metric::Counter { tags, .. } => tags,
            Metric::Gauge { tags, .. } => tags,
            Metric::Histogram { tags, .. } => tags,
            Metric::Set { tags, .. } => tags,
        }
    }

    pub fn is_mergeable(&self) -> bool {
        match self {
            Metric::Counter { .. } => true,
            Metric::Gauge { .. } => true,
            Metric::Histogram { .. } => false,
            Metric::Set { .. } => false,
        }
    }

    pub fn merge(&mut self, other: &Metric) {
        match (self, other) {
            (
                Metric::Counter {
                    ref mut val,
                    ref mut timestamp,
                    ..
                },
                Metric::Counter {
                    val: v,
                    timestamp: ts,
                    ..
                },
            ) => {
                *val += *v;
                *timestamp = *ts;
            }
            (
                Metric::Gauge {
                    ref mut val,
                    ref mut timestamp,
                    ..
                },
                Metric::Gauge {
                    val: v,
                    timestamp: ts,
                    ..
                },
            ) => {
                *val = *v;
                *timestamp = *ts;
            }
            _ => unimplemented!(),
        }
    }
}

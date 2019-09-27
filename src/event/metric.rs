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

    pub fn merge(&mut self, other: &Metric) {
        match (self, other) {
            (
                Metric::Counter {
                    ref mut val,
                    ref mut timestamp,
                    ref mut tags,
                    ..
                },
                Metric::Counter {
                    val: new_val,
                    timestamp: new_timestamp,
                    tags: new_tags,
                    ..
                },
            ) => {
                *val += *new_val;
                *timestamp = *new_timestamp;
                *tags = new_tags.clone();
            }
            (
                Metric::Gauge {
                    ref mut val,
                    direction: None,
                    ref mut timestamp,
                    ref mut tags,
                    ..
                },
                Metric::Gauge {
                    val: new_val,
                    timestamp: new_timestamp,
                    direction: new_direction,
                    tags: new_tags,
                    ..
                },
            ) => {
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
            _ => unimplemented!(),
        }
    }
}

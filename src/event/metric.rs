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
}

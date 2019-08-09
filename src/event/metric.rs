use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Metric {
    Counter {
        name: String,
        val: f64,
        timestamp: Option<DateTime<Utc>>,
    },
    Histogram {
        name: String,
        val: f64,
        sample_rate: u32,
        timestamp: Option<DateTime<Utc>>,
    },
    Gauge {
        name: String,
        val: f64,
        direction: Option<Direction>,
        timestamp: Option<DateTime<Utc>>,
    },
    Set {
        name: String,
        val: String,
        timestamp: Option<DateTime<Utc>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Direction {
    Plus,
    Minus,
}

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Metric {
    Counter {
        name: String,
        val: f64,
    },
    Histogram {
        name: String,
        val: f64,
        sample_rate: u32,
    },
    Gauge {
        name: String,
        val: f64,
        direction: Option<Direction>,
    },
    Set {
        name: String,
        val: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Direction {
    Plus,
    Minus,
}

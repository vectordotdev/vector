#[derive(Debug, Clone, PartialEq)]
pub enum Metric {
    Counter {
        name: String,
        val: f32,
    },
    Timer {
        name: String,
        val: f32,
        count: f32,
    },
    Gauge {
        name: String,
        val: f32,
        direction: Option<Direction>,
    },
    Set {
        name: String,
        val: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Plus,
    Minus,
}

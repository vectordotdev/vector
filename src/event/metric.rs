#[derive(Debug, Clone, PartialEq)]
pub enum Metric {
    Counter {
        name: String,
        val: u32,
        sampling: Option<f32>,
    },
    Timer {
        name: String,
        val: u32,
        sampling: Option<f32>,
    },
    Gauge {
        name: String,
        val: u32,
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

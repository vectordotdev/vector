use std::fmt::{self, Display, Formatter};
use std::ops::Deref;
use std::time::Instant;

/// A Histogram is a set of accumulator buckets numbered linearly
/// starting at zero. This storage will enlarge automatically as items
/// are added.
#[derive(Clone, Debug, Default)]
pub struct Histogram {
    totals: Vec<f64>,
}

impl Histogram {
    pub fn add(&mut self, index: usize, amount: f64) {
        if self.totals.len() <= index {
            self.totals
                .extend((self.totals.len()..index + 1).map(|_| 0.0));
        }
        self.totals[index] += amount;
    }

    /// The first bucket with a recorded value.
    pub fn min(&self) -> Option<usize> {
        self.totals
            .iter()
            .enumerate()
            .filter(|(_, total)| **total > 0.0)
            .map(|(i, _)| i)
            .next()
    }

    /// The last bucket with a recorded value.
    pub fn max(&self) -> Option<usize> {
        self.totals
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, total)| **total > 0.0)
            .map(|(i, _)| i)
            .next()
    }

    /// The total over all the weights
    pub fn total(&self) -> f64 {
        self.totals.iter().fold(0.0, |sum, &item| sum + item)
    }

    pub fn weighted_sum(&self) -> WeightedSum {
        self.totals
            .iter()
            .enumerate()
            .fold(WeightedSum::default(), |mut ws, (i, &total)| {
                ws.add(i as f64, total);
                ws
            })
    }

    /// The mean of the histogram is the mean of all the indices
    /// weighted by their accumulated value.
    pub fn mean(&self) -> Option<f64> {
        self.weighted_sum().mean()
    }
}

impl Display for Histogram {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "[min={}, max={}, mean={}, total={}]",
            self.min().unwrap_or(0),
            self.max().unwrap_or(0),
            self.mean().unwrap_or(0.0),
            self.total()
        )
    }
}

/// A TimeHistogram is a Histogram where the weights are equal to the
/// length of time since the last item was added. Time between the start
/// of the program and the first `add` is ignored.
#[derive(Clone, Debug, Default)]
pub struct TimeHistogram {
    histogram: Histogram,
    last_time: Option<Instant>,
}

impl TimeHistogram {
    pub fn add(&mut self, index: usize) {
        let now = Instant::now();
        if let Some(last) = self.last_time {
            let duration = (now - last).as_secs_f64();
            self.histogram.add(index, duration);
        }
        self.last_time = Some(now);
    }
}

impl Deref for TimeHistogram {
    type Target = Histogram;
    fn deref(&self) -> &Self::Target {
        &self.histogram
    }
}

impl Display for TimeHistogram {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        self.histogram.fmt(fmt)
    }
}

/// A LevelTimeHistogram is a convenience wrapper for a TimeHistogram
/// where the index is treated as a level which may be adjusted up or
/// down instead of being handled directly.
#[derive(Clone, Debug, Default)]
pub struct LevelTimeHistogram {
    level: usize,
    histogram: TimeHistogram,
}

impl LevelTimeHistogram {
    pub fn adjust(&mut self, adjustment: isize) -> usize {
        self.histogram.add(self.level);
        self.level = ((self.level as isize) + adjustment) as usize;
        self.level
    }

    pub fn level(&self) -> usize {
        self.level
    }
}

impl Deref for LevelTimeHistogram {
    type Target = TimeHistogram;
    fn deref(&self) -> &Self::Target {
        &self.histogram
    }
}

impl Display for LevelTimeHistogram {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        self.histogram.fmt(fmt)
    }
}

/// A WeightedSum contains an averaging mechanism that accepts a varying
/// weight at each point to be averaged, and biases the mean based on
/// those weights.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeightedSum {
    total: f64,
    weights: f64,
}

impl WeightedSum {
    pub fn add(&mut self, value: f64, weight: f64) {
        self.total += value * weight;
        self.weights += weight;
    }

    pub fn mean(&self) -> Option<f64> {
        if self.weights == 0.0 {
            None
        } else {
            Some(self.total / self.weights)
        }
    }
}

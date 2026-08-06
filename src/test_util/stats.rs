#![cfg(test)]

use std::{
    fmt::{self, Display, Formatter},
    ops::Deref,
    time::Instant,
};

use ordered_float::OrderedFloat;
use vector_lib::event::metric::Bucket;

#[derive(Copy, Clone, Debug, Default)]
pub struct HistogramStats {
    pub min: usize,  // The first bucket with a value
    pub max: usize,  // The last bucket with a value
    pub mode: usize, // The bucket with the highest value
    pub total: f64,  // The total over all the weights
    pub mean: f64,   // The mean of all indices weighted by their value
}

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

    pub fn stats(&self) -> Option<HistogramStats> {
        let (min, max, mode, sum) = self.totals.iter().enumerate().fold(
            (None, None, None, WeightedSum::default()),
            |(mut min, mut max, mut mode, mut sum), (i, &total)| {
                if total > 0.0 {
                    min = min.or(Some(i));
                    max = Some(i);
                    mode = Some(match mode {
                        None => (i, total),
                        Some((index, value)) => {
                            if value > total {
                                (index, value)
                            } else {
                                (i, total)
                            }
                        }
                    });
                }
                sum.add(i as f64, total);
                (min, max, mode, sum)
            },
        );
        min.map(|_| HistogramStats {
            min: min.unwrap(),
            max: max.unwrap(),
            mode: mode.unwrap().0,
            mean: sum.mean().unwrap(),
            total: sum.weights,
        })
    }
}

impl Display for Histogram {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self.stats() {
            None => write!(fmt, "[No stats]"),
            Some(stats) => write!(
                fmt,
                "[min={}, max={}, mode={}, mean={}, total={}]",
                stats.min, stats.max, stats.mode, stats.mean, stats.total
            ),
        }
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
    pub fn add(&mut self, index: usize, instant: Instant) {
        if let Some(last) = self.last_time {
            let duration = instant.saturating_duration_since(last).as_secs_f64();
            self.histogram.add(index, duration);
        }
        self.last_time = Some(instant);
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
    pub fn adjust(&mut self, adjustment: isize, instant: Instant) -> usize {
        self.histogram.add(self.level, instant);
        self.level = ((self.level as isize) + adjustment) as usize;
        self.level
    }

    pub const fn level(&self) -> usize {
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

/// A histogram with user-defined, variable-width buckets.
///
/// Values are only recorded into the bucket that the value is less than or equal to.
#[derive(Debug)]
pub struct VariableHistogram {
    buckets: Vec<(f64, u64)>,
    count: u64,
    sum: f64,
}

impl VariableHistogram {
    pub fn new(upper_limits: &[f64]) -> Self {
        let mut buckets = upper_limits.iter().map(|v| (*v, 0)).collect::<Vec<_>>();

        // Clear out any duplicate buckets, and sort them from smallest to largest.
        buckets.dedup_by_key(|(upper_limit, _)| OrderedFloat(*upper_limit));
        buckets.sort_by_key(|(upper_limit, _)| OrderedFloat(*upper_limit));

        Self {
            buckets,
            count: 0,
            sum: 0.0,
        }
    }

    pub fn record(&mut self, value: f64) {
        for (bound, count) in self.buckets.iter_mut() {
            if value <= *bound {
                *count += 1;
                break;
            }
        }

        self.count += 1;
        self.sum += value;
    }

    pub fn record_many(&mut self, values: &[f64]) {
        for value in values {
            self.record(*value);
        }
    }

    pub const fn count(&self) -> u64 {
        self.count
    }

    pub const fn sum(&self) -> f64 {
        self.sum
    }

    pub fn buckets(&self) -> Vec<Bucket> {
        self.buckets
            .iter()
            .map(|(upper_limit, count)| Bucket {
                upper_limit: *upper_limit,
                count: *count,
            })
            .collect()
    }
}

/// A WeightedSum contains an averaging mechanism that accepts a varying
/// weight at each point to be averaged, and biases the mean based on
/// those weights.
#[derive(Clone, Copy, Debug, Default)]
pub struct WeightedSum {
    total: f64,
    weights: f64,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WeightedSumStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl WeightedSum {
    pub fn add(&mut self, value: f64, weight: f64) {
        self.total += value * weight;
        self.weights += weight;
        self.max = Some(opt_max(self.max, value));
        self.min = Some(opt_min(self.min, value));
    }

    pub fn mean(&self) -> Option<f64> {
        if self.weights == 0.0 {
            None
        } else {
            Some(self.total / self.weights)
        }
    }

    pub fn stats(&self) -> Option<WeightedSumStats> {
        self.mean().map(|mean| WeightedSumStats {
            mean,
            min: self.min.unwrap(),
            max: self.max.unwrap(),
        })
    }
}

impl Display for WeightedSum {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self.stats() {
            None => write!(fmt, "[No stats]"),
            Some(stats) => write!(
                fmt,
                "[min={}, max={}, mean={}]",
                stats.min, stats.max, stats.mean
            ),
        }
    }
}

fn opt_max(opt: Option<f64>, value: f64) -> f64 {
    match opt {
        None => value,
        Some(s) if s > value => s,
        _ => value,
    }
}

fn opt_min(opt: Option<f64>, value: f64) -> f64 {
    match opt {
        None => value,
        Some(s) if s < value => s,
        _ => value,
    }
}

/// A TimeWeightedSum is a wrapper around WeightedSum that keeps track
/// of the last Instant a value was observed, and uses the duration
/// since that last observance to weight the added value.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimeWeightedSum {
    sum: WeightedSum,
    last_observation: Option<Instant>,
}

impl TimeWeightedSum {
    pub fn add(&mut self, value: f64, instant: Instant) {
        if let Some(then) = self.last_observation {
            let duration = instant.saturating_duration_since(then).as_secs_f64();
            self.sum.add(value, duration);
        }
        self.last_observation = Some(instant);
    }
}

impl Deref for TimeWeightedSum {
    type Target = WeightedSum;
    fn deref(&self) -> &Self::Target {
        &self.sum
    }
}

impl Display for TimeWeightedSum {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        self.sum.fmt(fmt)
    }
}

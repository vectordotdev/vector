#![allow(missing_docs)]

use std::sync::atomic::Ordering;

use crate::atomic::AtomicF64;

/// Exponentially Weighted Moving Average
#[derive(Clone, Copy, Debug)]
pub struct Ewma {
    average: Option<f64>,
    alpha: f64,
}

impl Ewma {
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        let average = None;
        Self { average, alpha }
    }

    #[must_use]
    pub const fn average(&self) -> Option<f64> {
        self.average
    }

    /// Update the current average and return it for convenience
    pub fn update(&mut self, point: f64) -> f64 {
        let average = match self.average {
            None => point,
            Some(avg) => point.mul_add(self.alpha, avg * (1.0 - self.alpha)),
        };
        self.average = Some(average);
        average
    }
}

/// Exponentially Weighted Moving Average that starts with a default average value
#[derive(Clone, Copy, Debug)]
pub struct EwmaDefault {
    average: f64,
    alpha: f64,
}

impl EwmaDefault {
    #[must_use]
    pub const fn new(alpha: f64, initial_value: f64) -> Self {
        Self {
            average: initial_value,
            alpha,
        }
    }

    #[must_use]
    pub const fn average(&self) -> f64 {
        self.average
    }

    /// Update the current average and return it for convenience
    pub fn update(&mut self, point: f64) -> f64 {
        self.average = point.mul_add(self.alpha, self.average * (1.0 - self.alpha));
        self.average
    }
}

/// Exponentially Weighted Moving Average with variance calculation
#[derive(Clone, Copy, Debug)]
pub struct EwmaVar {
    state: Option<MeanVariance>,
    alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeanVariance {
    pub mean: f64,
    pub variance: f64,
}

impl EwmaVar {
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        let state = None;
        Self { state, alpha }
    }

    #[must_use]
    pub const fn state(&self) -> Option<MeanVariance> {
        self.state
    }

    #[cfg(test)]
    #[must_use]
    pub fn average(&self) -> Option<f64> {
        self.state.map(|state| state.mean)
    }

    #[cfg(test)]
    #[must_use]
    pub fn variance(&self) -> Option<f64> {
        self.state.map(|state| state.variance)
    }

    /// Update the current average and variance, and return them for convenience
    pub fn update(&mut self, point: f64) -> MeanVariance {
        let (mean, variance) = match self.state {
            None => (point, 0.0),
            Some(state) => {
                let difference = point - state.mean;
                let increment = self.alpha * difference;
                (
                    state.mean + increment,
                    (1.0 - self.alpha) * difference.mul_add(increment, state.variance),
                )
            }
        };
        let state = MeanVariance { mean, variance };
        self.state = Some(state);
        state
    }
}

/// Simple unweighted arithmetic mean
#[derive(Clone, Copy, Debug, Default)]
pub struct Mean {
    mean: f64,
    count: usize,
}

impl Mean {
    /// Update the and return the current average
    #[expect(
        clippy::cast_precision_loss,
        reason = "We have to convert count to f64 for the calculation, it's okay to lose precision for very large counts."
    )]
    pub fn update(&mut self, point: f64) {
        self.count += 1;
        self.mean += (point - self.mean) / self.count as f64;
    }

    #[must_use]
    pub const fn average(&self) -> Option<f64> {
        match self.count {
            0 => None,
            _ => Some(self.mean),
        }
    }
}

/// Atomic EWMA that uses an `AtomicF64` to store the current average.
#[derive(Debug)]
pub struct AtomicEwma {
    average: AtomicF64,
    alpha: f64,
}

impl AtomicEwma {
    #[must_use]
    pub fn new(alpha: f64) -> Self {
        Self {
            average: AtomicF64::new(f64::NAN),
            alpha,
        }
    }

    pub fn update(&self, point: f64) -> f64 {
        let mut result = f64::NAN;
        self.average
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                let average = if current.is_nan() {
                    point
                } else {
                    point.mul_add(self.alpha, current * (1.0 - self.alpha))
                };
                result = average;
                average
            });
        result
    }

    pub fn average(&self) -> Option<f64> {
        let value = self.average.load(Ordering::Relaxed);
        if value.is_nan() { None } else { Some(value) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_update_works() {
        let mut mean = Mean::default();
        assert_eq!(mean.average(), None);
        mean.update(0.0);
        assert_eq!(mean.average(), Some(0.0));
        mean.update(2.0);
        assert_eq!(mean.average(), Some(1.0));
        mean.update(4.0);
        assert_eq!(mean.average(), Some(2.0));
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "none of the values will be rounded")]
    fn ewma_update_works() {
        let mut mean = Ewma::new(0.5);
        assert_eq!(mean.average(), None);
        assert_eq!(mean.update(2.0), 2.0);
        assert_eq!(mean.average(), Some(2.0));
        assert_eq!(mean.update(2.0), 2.0);
        assert_eq!(mean.average(), Some(2.0));
        assert_eq!(mean.update(1.0), 1.5);
        assert_eq!(mean.average(), Some(1.5));
        assert_eq!(mean.update(2.0), 1.75);
        assert_eq!(mean.average(), Some(1.75));

        assert_eq!(mean.average, Some(1.75));
    }

    #[test]
    fn ewma_variance_update_works() {
        let mut mean = EwmaVar::new(0.5);
        assert_eq!(mean.average(), None);
        assert_eq!(mean.variance(), None);
        mean.update(2.0);
        assert_eq!(mean.average(), Some(2.0));
        assert_eq!(mean.variance(), Some(0.0));
        mean.update(2.0);
        assert_eq!(mean.average(), Some(2.0));
        assert_eq!(mean.variance(), Some(0.0));
        mean.update(1.0);
        assert_eq!(mean.average(), Some(1.5));
        assert_eq!(mean.variance(), Some(0.25));
        mean.update(2.0);
        assert_eq!(mean.average(), Some(1.75));
        assert_eq!(mean.variance(), Some(0.1875));

        assert_eq!(
            mean.state,
            Some(MeanVariance {
                mean: 1.75,
                variance: 0.1875
            })
        );
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "none of the values will be rounded")]
    fn atomic_ewma_update_works() {
        let ewma = AtomicEwma::new(0.5);
        assert_eq!(ewma.average(), None);
        assert_eq!(ewma.update(2.0), 2.0);
        assert_eq!(ewma.average(), Some(2.0));
        assert_eq!(ewma.update(1.0), 1.5);
        assert_eq!(ewma.average(), Some(1.5));
    }
}

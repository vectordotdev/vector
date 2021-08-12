/// Exponentially Weighted Moving Average
#[derive(Clone, Copy, Debug)]
pub struct Ewma {
    average: Option<f64>,
    alpha: f64,
}

impl Ewma {
    pub fn new(alpha: f64) -> Self {
        let average = None;
        Self { average, alpha }
    }

    pub fn average(&self) -> Option<f64> {
        self.average
    }

    /// Update the current average and return it for convenience
    pub fn update(&mut self, point: f64) -> f64 {
        let average = match self.average {
            None => point,
            Some(avg) => point * self.alpha + avg * (1.0 - self.alpha),
        };
        self.average = Some(average);
        average
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
    pub fn new(alpha: f64) -> Self {
        let state = None;
        Self { state, alpha }
    }

    pub fn state(&self) -> Option<MeanVariance> {
        self.state
    }

    #[cfg(test)]
    pub fn average(&self) -> Option<f64> {
        self.state.map(|state| state.mean)
    }

    #[cfg(test)]
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
                    (1.0 - self.alpha) * (state.variance + difference * increment),
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
    pub fn update(&mut self, point: f64) {
        self.count += 1;
        self.mean += (point - self.mean) / self.count as f64;
    }

    pub fn average(&self) -> Option<f64> {
        match self.count {
            0 => None,
            _ => Some(self.mean),
        }
    }
}

/// Simple unweighted arithmetic mean with variance
#[derive(Clone, Copy, Debug, Default)]
pub struct MeanVar {
    count: usize,
    mean: f64,
    variance: f64,
}

impl MeanVar {
    /// Update the and return the current average
    pub fn update(&mut self, point: f64) {
        let count0 = self.count as f64;
        self.count += 1;
        // Ref: Knuth TAOCP vol 2, 3rd edition, page 232 and https://www.johndcook.com/blog/skewness_kurtosis/
        let delta = point - self.mean;
        let delta_n = delta / self.count as f64;
        self.mean += delta_n;
        self.variance += delta * delta_n * count0;
    }

    pub fn average(&self) -> Option<f64> {
        match self.count {
            0 => None,
            _ => Some(self.mean),
        }
    }

    pub fn variance(&self) -> Option<f64> {
        match self.count {
            0 | 1 => None,
            _ => Some(self.variance / (self.count - 1) as f64),
        }
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
    fn mean_variance_update_works() {
        let mut mean = MeanVar::default();
        assert_eq!(mean.average(), None);
        assert_eq!(mean.variance(), None);
        mean.update(0.0);
        assert_eq!(mean.average(), Some(0.0));
        assert_eq!(mean.variance(), None);
        mean.update(2.0);
        assert_eq!(mean.average(), Some(1.0));
        assert_eq!(mean.variance(), Some(2.0));
        mean.update(4.0);
        assert_eq!(mean.average(), Some(2.0));
        assert_eq!(mean.variance(), Some(4.0));
        assert_eq!(mean.count, 3);
        assert_eq!(mean.mean, 2.0);
        assert_eq!(mean.variance, 8.0);
    }

    #[test]
    fn ewma_update_works() {
        let mut mean = Ewma::new(0.5);
        assert_eq!(mean.average(), None);
        mean.update(2.0);
        assert_eq!(mean.average(), Some(2.0));
        mean.update(2.0);
        assert_eq!(mean.average(), Some(2.0));
        mean.update(1.0);
        assert_eq!(mean.average(), Some(1.5));
        mean.update(2.0);
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
}

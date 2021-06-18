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

/// Simple unweighted arithmetic mean
#[derive(Clone, Copy, Debug, Default)]
pub struct Mean {
    sum: f64,
    count: usize,
}

impl Mean {
    /// Update the and return the current average
    pub fn update(&mut self, point: f64) {
        self.sum += point;
        self.count += 1;
    }

    pub fn average(&self) -> Option<f64> {
        match self.count {
            0 => None,
            _ => Some(self.sum / self.count as f64),
        }
    }

    pub fn reset(&mut self) {
        self.sum = 0.0;
        self.count = 0;
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
        assert_eq!(mean.count, 3);
        assert_eq!(mean.sum, 6.0);
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
}

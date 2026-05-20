use std::time::Instant;

#[derive(Clone, Copy, Debug)]
struct State {
    average: f64,
    point: f64,
    reference: Instant,
}

/// Continuous-Time Exponentially Weighted Moving Average.
///
/// This is used to average values that are observed at irregular intervals but have a fixed value
/// between the observations, AKA a piecewise-constant signal sampled at change points. Instead of
/// an "alpha" parameter, this uses a "half-life" parameter which is the time it takes for the
/// average to decay to half of its value, measured in seconds.
#[derive(Clone, Copy, Debug)]
pub struct TimeEwma {
    state: Option<State>,
    half_life_seconds: f64,
}

impl TimeEwma {
    #[must_use]
    pub const fn new(half_life_seconds: f64) -> Self {
        Self {
            state: None,
            half_life_seconds,
        }
    }

    #[must_use]
    pub fn average(&self) -> Option<f64> {
        self.state.map(|state| state.average)
    }

    /// Update the current average and return it for convenience. Note that this average will "lag"
    /// the current observation because the new average is based on the previous point and the
    /// duration during which it was held constant. If the reference time is before the previous
    /// update, the update is ignored and the previous average is returned.
    pub fn update(&mut self, point: f64, reference: Instant) -> f64 {
        let average = match self.state {
            None => point,
            Some(state) => {
                if let Some(duration) = reference.checked_duration_since(state.reference) {
                    let k = (-duration.as_secs_f64() / self.half_life_seconds).exp2();
                    // The elapsed duration applies to the previously observed point, since that value
                    // was held constant between observations.
                    k * state.average + (1.0 - k) * state.point
                } else {
                    state.average
                }
            }
        };
        self.state = Some(State {
            average,
            point,
            reference,
        });
        average
    }
}

#[cfg(test)]
mod tests {
    use super::TimeEwma;
    use std::time::{Duration, Instant};

    #[test]
    #[expect(clippy::float_cmp, reason = "exact values for this test")]
    fn time_ewma_uses_previous_point_duration() {
        let mut ewma = TimeEwma::new(1.0);
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(1);
        let t2 = t1 + Duration::from_secs(1);

        assert_eq!(ewma.average(), None);
        assert_eq!(ewma.update(0.0, t0), 0.0);
        assert_eq!(ewma.average(), Some(0.0));
        assert_eq!(ewma.update(10.0, t1), 0.0);
        assert_eq!(ewma.average(), Some(0.0));
        assert_eq!(ewma.update(10.0, t2), 5.0);
        assert_eq!(ewma.average(), Some(5.0));
    }
}

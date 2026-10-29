/// Stateful sampler that retains events at a configured ratio.
///
/// Each call to [`Self::sample`] advances the sampler. Ratios must be between zero and one,
/// inclusive.
#[derive(Clone, Debug)]
pub struct RatioSampler {
    ratio: f64,
    value: f64,
}

impl RatioSampler {
    /// Creates a sampler that retains events at `ratio`.
    #[must_use]
    pub fn new(ratio: f64) -> Self {
        debug_assert!((0.0..=1.0).contains(&ratio));
        Self {
            ratio,
            value: 1.0 - ratio,
        }
    }

    /// Advances the sampler and returns whether the current event is retained.
    pub fn sample(&mut self) -> bool {
        let increment = self.value + self.ratio;
        self.value = if increment >= 1.0 {
            increment - 1.0
        } else {
            increment
        };
        increment >= 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::RatioSampler;

    #[test]
    fn retains_events_at_half_ratio() {
        let mut sampler = RatioSampler::new(0.5);
        let decisions = (0..6).map(|_| sampler.sample()).collect::<Vec<_>>();
        assert_eq!(decisions, [true, false, true, false, true, false]);
    }

    #[test]
    fn retains_every_event_at_full_ratio() {
        let mut sampler = RatioSampler::new(1.0);
        assert!((0..6).all(|_| sampler.sample()));
    }

    #[test]
    fn sampler_instances_advance_independently() {
        let mut first = RatioSampler::new(0.5);
        let mut second = RatioSampler::new(0.5);

        assert!(first.sample());
        assert!(!first.sample());
        assert!(second.sample());
    }
}

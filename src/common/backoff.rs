use std::time::Duration;

// `tokio-retry` crate
// MIT License
// Copyright (c) 2017 Sam Rijs
//
/// A retry strategy driven by exponential back-off.
///
/// The power corresponds to the number of past attempts.
#[derive(Debug, Clone)]
pub(crate) struct ExponentialBackoff {
    current: u64,
    base: u64,
    factor: u64,
    max_delay: Option<Duration>,
}

impl Default for ExponentialBackoff {
    /// `ExponentialBackoff` instance with sensible default values
    fn default() -> Self {
        Self::from_millis(2)
            .factor(250)
            .max_delay(Duration::from_secs(60))
    }
}

impl ExponentialBackoff {
    /// Constructs a new exponential back-off strategy,
    /// given a base duration in milliseconds.
    ///
    /// The resulting duration is calculated by taking the base to the `n`-th power,
    /// where `n` denotes the number of past attempts.
    pub(crate) const fn from_millis(base: u64) -> ExponentialBackoff {
        ExponentialBackoff {
            current: base,
            base,
            factor: 1u64,
            max_delay: None,
        }
    }

    /// A multiplicative factor that will be applied to the retry delay.
    ///
    /// For example, using a factor of `1000` will make each delay in units of seconds.
    ///
    /// Default factor is `1`.
    pub(crate) const fn factor(mut self, factor: u64) -> ExponentialBackoff {
        self.factor = factor;
        self
    }

    /// Apply a maximum delay. No retry delay will be longer than this `Duration`.
    pub(crate) const fn max_delay(mut self, duration: Duration) -> ExponentialBackoff {
        self.max_delay = Some(duration);
        self
    }

    /// Resents the exponential back-off strategy to its initial state.
    pub(crate) const fn reset(&mut self) {
        self.current = self.base;
    }
}

impl Iterator for ExponentialBackoff {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        // set delay duration by applying factor
        let duration = if let Some(duration) = self.current.checked_mul(self.factor) {
            Duration::from_millis(duration)
        } else {
            Duration::from_millis(u64::MAX)
        };

        // check if we reached max delay
        if let Some(ref max_delay) = self.max_delay
            && duration > *max_delay
        {
            return Some(*max_delay);
        }

        if let Some(next) = self.current.checked_mul(self.base) {
            self.current = next;
        } else {
            self.current = u64::MAX;
        }

        Some(duration)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff_sequence() {
        let mut backoff = ExponentialBackoff::default();

        let expected_delays = [
            Duration::from_millis(500), // 2 * 250
            Duration::from_secs(1),     // 4 * 250
            Duration::from_secs(2),     // 8 * 250
            Duration::from_secs(4),     // 16 * 250
            Duration::from_secs(8),     // 32 * 250
            Duration::from_secs(16),    // 64 * 250
            Duration::from_secs(32),    // 128 * 250
            Duration::from_secs(60),    // 256 * 250 = 64s, capped at 60
            Duration::from_secs(60),    // Should stay capped
        ];

        for expected in expected_delays.iter() {
            let actual = backoff.next().unwrap();
            assert_eq!(actual, *expected);
        }
    }

    #[test]
    fn test_backoff_reset() {
        let mut backoff = ExponentialBackoff::default();

        for _ in 0..2 {
            backoff.next();
        }
        assert_eq!(backoff.next().unwrap(), Duration::from_secs(2));
        backoff.reset();
        assert_eq!(backoff.next().unwrap(), Duration::from_millis(500));
    }
}

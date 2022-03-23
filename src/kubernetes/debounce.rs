//! Arbitrary signal debouncing logic.
//!
//! Call [`Debounce::signal`] multiple times within the debounce time window,
//! and the [`Debounce::debounced`] will be resolved only once.

use std::{future::pending, time::Duration};

use tokio::time::{sleep_until, Instant};

/// Provides an arbitrary signal debouncing.
pub struct Debounce {
    sequence_start: Option<Instant>,
    time: Duration,
}

impl Debounce {
    /// Create a new [`Debounce`].
    pub const fn new(time: Duration) -> Self {
        Self {
            sequence_start: None,
            time,
        }
    }

    /// Trigger a signal to debounce.
    pub fn signal(&mut self) {
        if self.sequence_start.is_none() {
            self.sequence_start = Some(Instant::now() + self.time);
        }
    }

    /// Debounced signal.
    ///
    /// This function resolves after a debounce timeout since the first signal
    /// in sequence expires.
    /// If there hasn't been a signal, or the debounce timeout isn't yet
    /// exhausted - the future will be in a pending state.
    pub async fn debounced(&mut self) {
        let sequence_start = match self.sequence_start {
            Some(val) => val,
            None => pending().await,
        };

        sleep_until(sequence_start).await;
        self.sequence_start = None;
    }

    /// This function exposes the state of the debounce logic.
    /// If this returns `false`, you shouldn't `poll` on [`Self::debounced`], as
    /// it's pending indefinitely.
    pub const fn is_debouncing(&self) -> bool {
        self.sequence_start.is_some()
    }
}

#[cfg(test)]
mod tests {
    use futures::{pin_mut, poll};

    use super::*;

    const TEST_DELAY_FRACTION: Duration = Duration::from_secs(60 * 60); // one hour
    const TEST_DELAY: Duration = Duration::from_secs(24 * 60 * 60); // one day

    #[tokio::test]
    async fn one_signal() {
        tokio::time::pause();

        let mut debounce = Debounce::new(TEST_DELAY);
        assert!(debounce.sequence_start.is_none());

        // Issue a signal.
        debounce.signal();
        assert!(debounce.sequence_start.is_some());

        {
            // Request debounced signal.
            let fut = debounce.debounced();
            pin_mut!(fut);

            // Shouldn't be available immediately.
            assert!(poll!(&mut fut).is_pending());

            // Simulate that we waited for some time, but not long enough for the
            // debounce to happen.
            tokio::time::advance(TEST_DELAY_FRACTION).await;

            // Still shouldn't be available.
            assert!(poll!(&mut fut).is_pending());

            // Then wait long enough for debounce timeout to pass.
            tokio::time::advance(TEST_DELAY * 2).await;

            // Should finally be available.
            assert!(poll!(&mut fut).is_ready());
        }

        assert!(debounce.sequence_start.is_none());

        tokio::time::resume();
    }

    #[tokio::test]
    async fn late_request() {
        tokio::time::pause();

        let mut debounce = Debounce::new(TEST_DELAY);
        assert!(debounce.sequence_start.is_none());

        // Issue a signal.
        debounce.signal();
        assert!(debounce.sequence_start.is_some());

        // Simulate that we waited long enough.
        tokio::time::advance(TEST_DELAY * 2).await;
        assert!(debounce.sequence_start.is_some());

        {
            // Request a debounced signal.
            let fut = debounce.debounced();
            pin_mut!(fut);

            // Should be available immediately.
            assert!(poll!(&mut fut).is_ready());
        }

        assert!(debounce.sequence_start.is_none());

        tokio::time::resume();
    }

    #[tokio::test]
    async fn multiple_signals() {
        tokio::time::pause();

        let mut debounce = Debounce::new(TEST_DELAY);
        assert!(debounce.sequence_start.is_none());

        debounce.signal();

        let first_signal_timestamp = debounce.sequence_start;
        assert!(first_signal_timestamp.is_some());

        debounce.signal();
        assert_eq!(debounce.sequence_start, first_signal_timestamp);

        tokio::time::advance(TEST_DELAY_FRACTION).await;

        debounce.signal();
        assert_eq!(debounce.sequence_start, first_signal_timestamp);

        {
            let fut = debounce.debounced();
            pin_mut!(fut);

            assert!(poll!(&mut fut).is_pending());

            tokio::time::advance(TEST_DELAY_FRACTION).await;

            assert!(poll!(&mut fut).is_pending());

            tokio::time::advance(TEST_DELAY * 2).await;

            assert!(poll!(&mut fut).is_ready());
        }

        assert!(debounce.sequence_start.is_none());

        tokio::time::resume();
    }

    #[tokio::test]
    async fn sequence() {
        tokio::time::pause();

        let mut debounce = Debounce::new(TEST_DELAY);
        assert!(debounce.sequence_start.is_none());

        debounce.signal();

        let first_signal_timestamp = debounce.sequence_start;
        assert!(first_signal_timestamp.is_some());

        debounce.signal();
        assert_eq!(debounce.sequence_start, first_signal_timestamp);

        tokio::time::advance(TEST_DELAY_FRACTION).await;

        debounce.signal();
        assert_eq!(debounce.sequence_start, first_signal_timestamp);

        {
            let fut = debounce.debounced();
            pin_mut!(fut);

            assert!(poll!(&mut fut).is_pending());

            tokio::time::advance(TEST_DELAY * 2).await;

            assert!(poll!(&mut fut).is_ready());
        }

        assert!(debounce.sequence_start.is_none());

        debounce.signal();

        let second_signal_timestamp = debounce.sequence_start;
        assert!(second_signal_timestamp.is_some());
        assert_ne!(second_signal_timestamp, first_signal_timestamp);

        {
            let fut = debounce.debounced();
            pin_mut!(fut);

            assert!(poll!(&mut fut).is_pending());

            tokio::time::advance(TEST_DELAY * 2).await;

            assert!(poll!(&mut fut).is_ready());
        }

        assert!(debounce.sequence_start.is_none());

        tokio::time::resume();
    }

    #[tokio::test]
    async fn is_debouncing() {
        tokio::time::pause();

        let mut debounce = Debounce::new(TEST_DELAY);
        assert!(!debounce.is_debouncing());

        debounce.signal();
        assert!(debounce.is_debouncing());

        tokio::time::advance(TEST_DELAY * 2).await;
        assert!(debounce.is_debouncing());

        debounce.debounced().await;
        assert!(!debounce.is_debouncing(),);

        tokio::time::resume();
    }
}

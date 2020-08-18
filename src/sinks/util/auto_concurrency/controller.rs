use super::instant_now;
use super::semaphore::ShrinkableSemaphore;
use crate::emit;
use crate::internal_events::{
    AutoConcurrencyAveragedRtt, AutoConcurrencyInFlight, AutoConcurrencyLimit,
    AutoConcurrencyObservedRtt,
};
use crate::sinks::util::retries2::RetryLogic;
#[cfg(test)]
use crate::test_util::stats::{TimeHistogram, TimeWeightedSum};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;
use tower03::timeout::error::Elapsed;

// This value was picked as a reasonable default while we ensure the
// viability of the system. This value may need adjustment if later
// analysis descovers we need higher or lower weighting on past RTT
// weighting.
const EWMA_ALPHA: f64 = 0.5;

// This was picked as a reasonable default threshold ratio to avoid
// dropping concurrency too aggressively when there is fluctuation in
// the RTT measurements.
const THRESHOLD_RATIO: f64 = 0.05;

/// Shared class for `tokio::sync::Semaphore` that manages adjusting the
/// semaphore size and other associated data.
#[derive(Clone, Debug)]
pub(super) struct Controller<L> {
    semaphore: Arc<ShrinkableSemaphore>,
    in_flight_limit: Option<usize>,
    logic: L,
    pub(super) inner: Arc<Mutex<Inner>>,
    #[cfg(test)]
    pub(super) stats: Arc<Mutex<ControllerStatistics>>,
}

#[derive(Debug)]
pub(super) struct Inner {
    pub(super) current_limit: usize,
    in_flight: usize,
    past_rtt: EWMA,
    next_update: Instant,
    current_rtt: Mean,
    had_back_pressure: bool,
    reached_limit: bool,
}

#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct ControllerStatistics {
    pub(super) in_flight: TimeHistogram,
    pub(super) concurrency_limit: TimeHistogram,
    pub(super) observed_rtt: TimeWeightedSum,
    pub(super) averaged_rtt: TimeWeightedSum,
}

impl<L> Controller<L> {
    pub(super) fn new(in_flight_limit: Option<usize>, logic: L) -> Self {
        // If an in_flight_limit is specified, it becomse both the
        // current limit and the maximum, effectively bypassing all the
        // mechanisms. Otherwise, the current limit is set to 1 and the
        // maximum to MAX_CONCURRENCY.
        let current_limit = in_flight_limit.unwrap_or(1);
        Self {
            semaphore: Arc::new(ShrinkableSemaphore::new(current_limit)),
            in_flight_limit,
            logic,
            inner: Arc::new(Mutex::new(Inner {
                current_limit,
                in_flight: 0,
                past_rtt: Default::default(),
                next_update: instant_now(),
                current_rtt: Default::default(),
                had_back_pressure: false,
                reached_limit: false,
            })),
            #[cfg(test)]
            stats: Arc::new(Mutex::new(ControllerStatistics::default())),
        }
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");
        if inner.in_flight >= inner.current_limit {
            inner.reached_limit = true;
        }
        Arc::clone(&self.semaphore).acquire()
    }

    pub(super) fn start_request(&self) {
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        #[cfg(test)]
        {
            let mut stats = self.stats.lock().expect("Stats mutex is poisoned");
            stats.in_flight.add(inner.in_flight, instant_now());
        }

        inner.in_flight += 1;
        emit!(AutoConcurrencyInFlight {
            in_flight: inner.in_flight as u64
        });
    }

    fn adjust_to_back_pressure(&self, start: Instant, is_back_pressure: bool) {
        let now = instant_now();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        let rtt = now.saturating_duration_since(start);
        emit!(AutoConcurrencyObservedRtt { rtt });
        let rtt = rtt.as_secs_f64();

        if is_back_pressure {
            inner.had_back_pressure = true;
        }

        #[cfg(test)]
        let mut stats = self.stats.lock().expect("Stats mutex is poisoned");

        #[cfg(test)]
        {
            stats.observed_rtt.add(rtt, now);
            stats.in_flight.add(inner.in_flight, now);
        }

        inner.in_flight -= 1;
        emit!(AutoConcurrencyInFlight {
            in_flight: inner.in_flight as u64
        });

        let current_rtt = inner.current_rtt.update(rtt);
        let past_rtt = inner.past_rtt.average();

        if past_rtt == 0.0 {
            // No past measurements, set up initial values.
            inner.past_rtt.update(current_rtt);
            inner.next_update = now + Duration::from_secs_f64(current_rtt);
        } else if past_rtt > 0.0 && now >= inner.next_update {
            #[cfg(test)]
            {
                stats.averaged_rtt.add(current_rtt, now);
                stats.concurrency_limit.add(inner.current_limit, now);
                drop(stats); // Drop the stats lock a little earlier on this path
            }

            emit!(AutoConcurrencyAveragedRtt {
                rtt: Duration::from_secs_f64(current_rtt)
            });

            let threshold = past_rtt * THRESHOLD_RATIO;

            // Only manage the concurrency if in_flight_limit was set to "auto"
            if self.in_flight_limit.is_none() {
                // Normal quick responses trigger an increase in the
                // concurrency limit. Note that we only check this if we had
                // requests to go beyond the current limit to prevent
                // increasing the limit beyond what we have evidence for.
                if inner.current_limit < super::MAX_CONCURRENCY
                    && inner.reached_limit
                    && !inner.had_back_pressure
                    && current_rtt <= past_rtt
                {
                    // Increase (additive) the current concurrency limit
                    self.semaphore.add_permits(1);
                    inner.current_limit += 1;
                }
                // Back pressure responses, either explicit or implicit due
                // to increasing response times, trigger a decrease in the
                // concurrency limit.
                else if inner.current_limit > 1
                    && (inner.had_back_pressure || current_rtt >= past_rtt + threshold)
                {
                    // Decrease (multiplicative) the current concurrency limit
                    let to_forget = inner.current_limit / 2;
                    self.semaphore.forget_permits(to_forget);
                    inner.current_limit -= to_forget;
                }
                emit!(AutoConcurrencyLimit {
                    concurrency: inner.current_limit as u64,
                    reached_limit: inner.reached_limit,
                    had_back_pressure: inner.had_back_pressure,
                    current_rtt: Duration::from_secs_f64(current_rtt),
                    past_rtt: Duration::from_secs_f64(past_rtt),
                });
            }

            // Reset values for next interval
            let new_avg = inner.past_rtt.update(current_rtt);
            inner.next_update = now + Duration::from_secs_f64(new_avg);
            inner.current_rtt.reset();
            inner.had_back_pressure = false;
            inner.reached_limit = false;
        }
    }
}

impl<L> Controller<L>
where
    L: RetryLogic,
{
    pub(super) fn adjust_to_response(
        &self,
        start: Instant,
        response: &Result<L::Response, crate::Error>,
    ) {
        let is_back_pressure = match response {
            Ok(_) => false,
            Err(err) => {
                if let Some(err) = err.downcast_ref::<L::Error>() {
                    self.logic.is_retriable_error(err)
                } else if err.downcast_ref::<Elapsed>().is_some() {
                    true
                } else {
                    unreachable!("Unhandled error response! {:?}", err)
                }
            }
        };
        self.adjust_to_back_pressure(start, is_back_pressure)
    }
}

/// Exponentially Weighted Moving Average
#[derive(Clone, Copy, Debug, Default)]
struct EWMA {
    average: f64,
}

impl EWMA {
    fn average(&self) -> f64 {
        self.average
    }

    /// Update and return the current average
    fn update(&mut self, point: f64) -> f64 {
        self.average = match self.average {
            avg if avg == 0.0 => point,
            avg => point * EWMA_ALPHA + avg * (1.0 - EWMA_ALPHA),
        };
        self.average
    }
}

/// Simple unweighted arithmetic mean
#[derive(Clone, Copy, Debug, Default)]
struct Mean {
    sum: f64,
    count: usize,
}

impl Mean {
    /// Update and return the current average
    fn update(&mut self, point: f64) -> f64 {
        self.sum += point;
        self.count += 1;
        self.sum / self.count as f64
    }

    fn reset(&mut self) {
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
        assert_eq!(mean.update(0.0), 0.0);
        assert_eq!(mean.update(2.0), 1.0);
        assert_eq!(mean.update(4.0), 2.0);
        assert_eq!(mean.count, 3);
        assert_eq!(mean.sum, 6.0);
    }

    #[test]
    fn ewma_update_works() {
        let mut mean = EWMA::default();
        assert_eq!(mean.average, 0.0);
        assert_eq!(mean.update(2.0), 2.0);
        assert_eq!(mean.update(2.0), 2.0);
        assert_eq!(mean.update(1.0), 1.5);
        assert_eq!(mean.update(2.0), 1.75);
        assert_eq!(mean.average, 1.75);
    }
}

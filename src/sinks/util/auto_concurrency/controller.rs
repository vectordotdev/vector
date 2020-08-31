use super::instant_now;
use super::semaphore::ShrinkableSemaphore;
use crate::emit;
use crate::internal_events::{
    AutoConcurrencyAveragedRtt, AutoConcurrencyInFlight, AutoConcurrencyLimit,
    AutoConcurrencyObservedRtt,
};
use crate::sinks::util::retries::RetryLogic;
#[cfg(test)]
use crate::test_util::stats::{TimeHistogram, TimeWeightedSum};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;
use tower::timeout::error::Elapsed;

// This value was picked as a reasonable default while we ensure the
// viability of the system. This value may need adjustment if later
// analysis discovers we need higher or lower weighting on past RTT
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

    /// Adjust the controller to a response, based on type of response
    /// given (backpressure or not) and if it should be used as a valid
    /// RTT measurement.
    fn adjust_to_response_inner(&self, start: Instant, is_back_pressure: bool, use_rtt: bool) {
        let now = instant_now();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        let rtt = now.saturating_duration_since(start);
        if use_rtt {
            emit!(AutoConcurrencyObservedRtt { rtt });
        }
        let rtt = rtt.as_secs_f64();

        if is_back_pressure {
            inner.had_back_pressure = true;
        }

        #[cfg(test)]
        let mut stats = self.stats.lock().expect("Stats mutex is poisoned");

        #[cfg(test)]
        {
            if use_rtt {
                stats.observed_rtt.add(rtt, now);
            }
            stats.in_flight.add(inner.in_flight, now);
        }

        inner.in_flight -= 1;
        emit!(AutoConcurrencyInFlight {
            in_flight: inner.in_flight as u64
        });

        if use_rtt {
            inner.current_rtt.update(rtt);
        }
        let current_rtt = inner.current_rtt.average();

        match inner.past_rtt.average() {
            None => {
                // No past measurements, set up initial values.
                if let Some(current_rtt) = current_rtt {
                    inner.past_rtt.update(current_rtt);
                    inner.next_update = now + Duration::from_secs_f64(current_rtt);
                }
            }
            Some(mut past_rtt) => {
                if now >= inner.next_update {
                    #[cfg(test)]
                    {
                        if let Some(current_rtt) = current_rtt {
                            stats.averaged_rtt.add(current_rtt, now);
                        }
                        stats.concurrency_limit.add(inner.current_limit, now);
                        drop(stats); // Drop the stats lock a little earlier on this path
                    }

                    if let Some(current_rtt) = current_rtt {
                        emit!(AutoConcurrencyAveragedRtt {
                            rtt: Duration::from_secs_f64(current_rtt)
                        });
                    }

                    // Only manage the concurrency if in_flight_limit was set to "auto"
                    if self.in_flight_limit.is_none() {
                        self.manage_limit(&mut inner, past_rtt, current_rtt);
                    }

                    // Reset values for next interval
                    if let Some(current_rtt) = current_rtt {
                        past_rtt = inner.past_rtt.update(current_rtt);
                    }
                    inner.next_update = now + Duration::from_secs_f64(past_rtt);
                    inner.current_rtt.reset();
                    inner.had_back_pressure = false;
                    inner.reached_limit = false;
                }
            }
        }
    }

    fn manage_limit(&self, inner: &mut MutexGuard<Inner>, past_rtt: f64, current_rtt: Option<f64>) {
        let threshold = past_rtt * THRESHOLD_RATIO;

        // Normal quick responses trigger an increase in the
        // concurrency limit. Note that we only check this if we had
        // requests to go beyond the current limit to prevent
        // increasing the limit beyond what we have evidence for.
        if inner.current_limit < super::MAX_CONCURRENCY
            && inner.reached_limit
            && !inner.had_back_pressure
            && current_rtt.is_some()
            && current_rtt.unwrap() <= past_rtt
        {
            // Increase (additive) the current concurrency limit
            self.semaphore.add_permits(1);
            inner.current_limit += 1;
        }
        // Back pressure responses, either explicit or implicit due
        // to increasing response times, trigger a decrease in the
        // concurrency limit.
        else if inner.current_limit > 1
            && (inner.had_back_pressure || current_rtt.unwrap_or(0.0) >= past_rtt + threshold)
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
            current_rtt: current_rtt.map(Duration::from_secs_f64),
            past_rtt: Duration::from_secs_f64(past_rtt),
        });
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
        // Only adjust to the RTT when the request was successfully processed.
        let use_rtt = response.is_ok();
        self.adjust_to_response_inner(start, is_back_pressure, use_rtt)
    }
}

/// Exponentially Weighted Moving Average
#[derive(Clone, Copy, Debug, Default)]
struct EWMA {
    average: Option<f64>,
}

impl EWMA {
    fn average(&self) -> Option<f64> {
        self.average
    }

    /// Update the current average and return it for convenience
    fn update(&mut self, point: f64) -> f64 {
        let average = match self.average {
            None => point,
            Some(avg) => point * EWMA_ALPHA + avg * (1.0 - EWMA_ALPHA),
        };
        self.average = Some(average);
        average
    }
}

/// Simple unweighted arithmetic mean
#[derive(Clone, Copy, Debug, Default)]
struct Mean {
    sum: f64,
    count: usize,
}

impl Mean {
    /// Update the and return the current average
    fn update(&mut self, point: f64) {
        self.sum += point;
        self.count += 1;
    }

    fn average(&self) -> Option<f64> {
        match self.count {
            0 => None,
            _ => Some(self.sum / self.count as f64),
        }
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
        let mut mean = EWMA::default();
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

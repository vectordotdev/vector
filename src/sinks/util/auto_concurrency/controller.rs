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
use std::cmp::max;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::OwnedSemaphorePermit;
use tower03::timeout::error::Elapsed;

const EWMA_ALPHA: f64 = 0.5;
const THRESHOLD_RATIO: f64 = 0.05;

/// Shared class for `tokio::sync::Semaphore` that manages adjusting the
/// semaphore size and other associated data.
#[derive(Clone, Debug)]
pub(super) struct Controller<L> {
    semaphore: Arc<ShrinkableSemaphore>,
    max: usize,
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
    pub(super) fn new(max: usize, logic: L, current_limit: usize) -> Self {
        Self {
            semaphore: Arc::new(ShrinkableSemaphore::new(current_limit)),
            max,
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
        self.semaphore.clone().acquire()
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
        #[cfg(test)]
        let mut stats = self.stats.lock().expect("Stats mutex is poisoned");

        let rtt = now.saturating_duration_since(start);
        emit!(AutoConcurrencyObservedRtt { rtt });
        let rtt = rtt.as_secs_f64();
        #[cfg(test)]
        stats.observed_rtt.add(rtt, now);

        if is_back_pressure {
            inner.had_back_pressure = true;
        }

        #[cfg(test)]
        stats.in_flight.add(inner.in_flight, now);
        inner.in_flight -= 1;
        emit!(AutoConcurrencyInFlight {
            in_flight: inner.in_flight as u64
        });

        let rtt = inner.current_rtt.update(rtt);
        let avg = inner.past_rtt.average();

        if avg == 0.0 {
            // No past measurements, set up initial values.
            inner.past_rtt.update(rtt);
            inner.next_update = now + Duration::from_secs_f64(rtt);
        } else if avg > 0.0 && now >= inner.next_update {
            emit!(AutoConcurrencyAveragedRtt {
                rtt: Duration::from_secs_f64(rtt)
            });
            #[cfg(test)]
            {
                stats.averaged_rtt.add(rtt, now);
                stats.concurrency_limit.add(inner.current_limit, now);
            }

            let threshold = avg * THRESHOLD_RATIO;

            // Back pressure responses, either explicit or implicit due
            // to increasing response times, trigger a decrease in the
            // concurrency limit.
            if inner.current_limit > 1 && (inner.had_back_pressure || rtt >= avg + threshold) {
                // Decrease (multiplicative) the current concurrency limit
                let to_forget = inner.current_limit / 2;
                self.semaphore.forget_permits(to_forget);
                inner.current_limit -= to_forget;
            }
            // Normal quick responses trigger an increase in the
            // concurrency limit. Note that we only check this if we had
            // requests to go beyond the current limit to prevent
            // increasing the limit beyond what we have evidence for.
            else if inner.current_limit < self.max
                && inner.reached_limit
                && !inner.had_back_pressure
                && rtt <= avg
            {
                // Increase (additive) the current concurrency limit
                self.semaphore.add_permits(1);
                inner.current_limit += 1;
            }
            emit!(AutoConcurrencyLimit {
                concurrency: inner.current_limit as u64,
            });

            // Reset values for next interval
            let new_avg = inner.past_rtt.update(rtt);
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
                    panic!("Unhandled error response! {:?}", err)
                }
            }
        };
        self.adjust_to_back_pressure(start, is_back_pressure)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct EWMA {
    average: f64,
}

impl EWMA {
    fn average(&self) -> f64 {
        self.average
    }

    fn update(&mut self, point: f64) -> f64 {
        self.average = match self.average {
            avg if avg == 0.0 => point,
            avg => point * EWMA_ALPHA + avg * (1.0 - EWMA_ALPHA),
        };
        self.average
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Mean {
    sum: f64,
    count: usize,
}

impl Mean {
    fn update(&mut self, point: f64) -> f64 {
        self.sum += point;
        self.count += 1;
        // Return current average
        self.sum / max(self.count, 1) as f64
    }

    fn reset(&mut self) {
        self.sum = 0.0;
        self.count = 0;
    }
}

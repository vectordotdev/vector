use std::{
    future::Future,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use tokio::sync::OwnedSemaphorePermit;
use tower::timeout::error::Elapsed;
use vector_lib::internal_event::{InternalEventHandle as _, Registered};

use super::{instant_now, semaphore::ShrinkableSemaphore, AdaptiveConcurrencySettings};
#[cfg(test)]
use crate::test_util::stats::{TimeHistogram, TimeWeightedSum};
use crate::{
    http::HttpError,
    internal_events::{
        AdaptiveConcurrencyAveragedRtt, AdaptiveConcurrencyInFlight, AdaptiveConcurrencyLimit,
        AdaptiveConcurrencyLimitData, AdaptiveConcurrencyObservedRtt,
    },
    sinks::util::retries::{RetryAction, RetryLogic},
    stats::{EwmaVar, Mean, MeanVariance},
};

/// Shared class for `tokio::sync::Semaphore` that manages adjusting the
/// semaphore size and other associated data.
#[derive(Clone)]
pub(super) struct Controller<L> {
    semaphore: Arc<ShrinkableSemaphore>,
    concurrency: Option<usize>,
    settings: AdaptiveConcurrencySettings,
    logic: L,
    pub(super) inner: Arc<Mutex<Inner>>,
    #[cfg(test)]
    pub(super) stats: Arc<Mutex<ControllerStatistics>>,

    limit: Registered<AdaptiveConcurrencyLimit>,
    in_flight: Registered<AdaptiveConcurrencyInFlight>,
    observed_rtt: Registered<AdaptiveConcurrencyObservedRtt>,
    averaged_rtt: Registered<AdaptiveConcurrencyAveragedRtt>,
}

#[derive(Debug)]
pub(super) struct Inner {
    pub(super) current_limit: usize,
    in_flight: usize,
    past_rtt: EwmaVar,
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
    pub(super) fn new(
        concurrency: Option<usize>,
        settings: AdaptiveConcurrencySettings,
        logic: L,
    ) -> Self {
        // If a `concurrency` is specified, it becomes both the
        // current limit and the maximum, effectively bypassing all the
        // mechanisms. Otherwise, the current limit is set to 1 and the
        // maximum to `settings.max_concurrency_limit`.
        let current_limit = concurrency.unwrap_or(settings.initial_concurrency);
        Self {
            semaphore: Arc::new(ShrinkableSemaphore::new(current_limit)),
            concurrency,
            settings,
            logic,
            inner: Arc::new(Mutex::new(Inner {
                current_limit,
                in_flight: 0,
                past_rtt: EwmaVar::new(settings.ewma_alpha),
                next_update: instant_now(),
                current_rtt: Default::default(),
                had_back_pressure: false,
                reached_limit: false,
            })),
            #[cfg(test)]
            stats: Arc::new(Mutex::new(ControllerStatistics::default())),
            limit: register!(AdaptiveConcurrencyLimit),
            in_flight: register!(AdaptiveConcurrencyInFlight),
            observed_rtt: register!(AdaptiveConcurrencyObservedRtt),
            averaged_rtt: register!(AdaptiveConcurrencyAveragedRtt),
        }
    }

    /// An estimate of current load on service managed by this controller.
    ///
    /// 0.0 is no load, while 1.0 is max load.
    pub(super) fn load(&self) -> f64 {
        let inner = self.inner.lock().expect("Controller mutex is poisoned");
        if inner.current_limit > 0 {
            inner.in_flight as f64 / inner.current_limit as f64
        } else {
            1.0
        }
    }

    pub(super) fn acquire(&self) -> impl Future<Output = OwnedSemaphorePermit> + Send + 'static {
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
        if inner.in_flight >= inner.current_limit {
            inner.reached_limit = true;
        }

        self.in_flight.emit(inner.in_flight as u64);
    }

    /// Adjust the controller to a response, based on type of response
    /// given (backpressure or not) and if it should be used as a valid
    /// RTT measurement.
    fn adjust_to_response_inner(&self, start: Instant, is_back_pressure: bool, use_rtt: bool) {
        let now = instant_now();
        let mut inner = self.inner.lock().expect("Controller mutex is poisoned");

        let rtt = now.saturating_duration_since(start);
        if use_rtt {
            self.observed_rtt.emit(rtt);
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
        self.in_flight.emit(inner.in_flight as u64);

        if use_rtt {
            inner.current_rtt.update(rtt);
        }
        let current_rtt = inner.current_rtt.average();

        // When the RTT values are all exactly the same, as for the
        // "constant link" test, the average calculation above produces
        // results either the exact value or that value plus epsilon,
        // depending on the number of samples. This ends up throttling
        // aggressively due to the high side falling outside of the
        // calculated deviance. Rounding these values forces the
        // differences to zero.
        #[cfg(test)]
        let current_rtt = current_rtt.map(|c| (c * 1000000.0).round() / 1000000.0);

        match inner.past_rtt.state() {
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
                        self.averaged_rtt.emit(Duration::from_secs_f64(current_rtt));
                    }

                    // Only manage the concurrency if `concurrency` was set to "adaptive"
                    if self.concurrency.is_none() {
                        self.manage_limit(&mut inner, past_rtt, current_rtt);
                    }

                    // Reset values for next interval
                    if let Some(current_rtt) = current_rtt {
                        past_rtt = inner.past_rtt.update(current_rtt);
                    }
                    inner.next_update = now + Duration::from_secs_f64(past_rtt.mean);
                    inner.current_rtt = Default::default();
                    inner.had_back_pressure = false;
                    inner.reached_limit = false;
                }
            }
        }
    }

    fn manage_limit(
        &self,
        inner: &mut MutexGuard<Inner>,
        past_rtt: MeanVariance,
        current_rtt: Option<f64>,
    ) {
        let past_rtt_deviation = past_rtt.variance.sqrt();
        let threshold = past_rtt_deviation * self.settings.rtt_deviation_scale;

        // Normal quick responses trigger an increase in the
        // concurrency limit. Note that we only check this if we had
        // requests to go beyond the current limit to prevent
        // increasing the limit beyond what we have evidence for.
        if inner.current_limit < self.settings.max_concurrency_limit
            && inner.reached_limit
            && !inner.had_back_pressure
            && current_rtt.is_some()
            && current_rtt.unwrap() <= past_rtt.mean
        {
            // Increase (additive) the current concurrency limit
            self.semaphore.add_permits(1);
            inner.current_limit += 1;
        }
        // Back pressure responses, either explicit or implicit due
        // to increasing response times, trigger a decrease in the
        // concurrency limit.
        else if inner.current_limit > 1
            && (inner.had_back_pressure || current_rtt.unwrap_or(0.0) >= past_rtt.mean + threshold)
        {
            // Decrease (multiplicative) the current concurrency limit
            let to_forget = inner.current_limit
                - (inner.current_limit as f64 * self.settings.decrease_ratio) as usize;
            self.semaphore.forget_permits(to_forget);
            inner.current_limit -= to_forget;
        }
        self.limit.emit(AdaptiveConcurrencyLimitData {
            concurrency: inner.current_limit as u64,
            reached_limit: inner.reached_limit,
            had_back_pressure: inner.had_back_pressure,
            current_rtt: current_rtt.map(Duration::from_secs_f64),
            past_rtt: Duration::from_secs_f64(past_rtt.mean),
            past_rtt_deviation: Duration::from_secs_f64(past_rtt_deviation),
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
        // It would be better to avoid generating the string in Retry(_)
        // just to throw it away here, but it's probably not worth the
        // effort.
        let response_action = response
            .as_ref()
            .map(|resp| self.logic.should_retry_response(resp));
        let is_back_pressure = match &response_action {
            Ok(action) => matches!(action, RetryAction::Retry(_)),
            Err(error) => {
                if let Some(error) = error.downcast_ref::<L::Error>() {
                    self.logic.is_retriable_error(error)
                } else if error.downcast_ref::<Elapsed>().is_some() {
                    true
                } else if error.downcast_ref::<HttpError>().is_some() {
                    // HTTP protocol-level errors are not backpressure
                    false
                } else {
                    warn!(
                        message = "Unhandled error response.",
                        %error,
                        internal_log_rate_limit = true
                    );
                    false
                }
            }
        };
        // Only adjust to the RTT when the request was successfully processed.
        let use_rtt = matches!(response_action, Ok(RetryAction::Successful));
        self.adjust_to_response_inner(start, is_back_pressure, use_rtt)
    }
}

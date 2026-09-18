use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use vector_lib::stats::EwmaDefault;

const EWMA_WEIGHT: f64 = 0.1;
const MINIMUM_PERMITS: usize = 2;

pub const MAX_IN_FLIGHT_EVENTS_TARGET: usize = 100_000;

pub struct RequestLimiterPermit {
    semaphore_permit: Option<OwnedSemaphorePermit>,
    request_limiter_data: Arc<Mutex<RequestLimiterData>>,
}

impl RequestLimiterPermit {
    pub fn decoding_finished(&self, num_events: usize) {
        let mut request_limiter_data = self.request_limiter_data.lock().unwrap();
        request_limiter_data.update_average(num_events);
    }
}

impl Drop for RequestLimiterPermit {
    fn drop(&mut self) {
        if let Ok(mut request_limiter_data) = self.request_limiter_data.lock() {
            let target = request_limiter_data.target_requests_in_flight();
            let current = request_limiter_data.total_permits;

            match target.cmp(&current) {
                Ordering::Greater => request_limiter_data.increase_permits(),
                Ordering::Equal => {
                    // Only release the current permit when the inner permit is dropped.
                }
                Ordering::Less => {
                    let permit = self.semaphore_permit.take().unwrap();
                    request_limiter_data.decrease_permits(permit);
                }
            }
        }
    }
}

struct RequestLimiterData {
    event_limit_target: usize,
    minimum_permits: usize,
    total_permits: usize,
    average_request_size: EwmaDefault,
    semaphore: Arc<Semaphore>,
    max_requests: usize,
}

impl RequestLimiterData {
    fn update_average(&mut self, num_events: usize) {
        if num_events > 0 {
            self.average_request_size.update(num_events as f64);
        }
    }

    fn target_requests_in_flight(&self) -> usize {
        let target = (self.event_limit_target as f64) / self.average_request_size.average();
        #[allow(clippy::manual_clamp)]
        (target as usize)
            .max(self.minimum_permits)
            .min(self.max_requests)
    }

    fn increase_permits(&mut self) {
        if self.total_permits < self.max_requests {
            self.total_permits += 1;
            self.semaphore.add_permits(1);
        }
    }

    fn decrease_permits(&mut self, permit: OwnedSemaphorePermit) {
        if self.total_permits > self.minimum_permits {
            permit.forget();
            self.total_permits -= 1;
        }
    }
}

#[derive(Clone)]
pub struct RequestLimiter {
    semaphore: Arc<Semaphore>,
    data: Arc<Mutex<RequestLimiterData>>,
}

impl RequestLimiter {
    /// Creates a limiter targeting `event_limit_target` in-flight events, capped at
    /// `max_requests` concurrent requests.
    ///
    /// The number of events in a request is not known until after decoding, so the event target is
    /// adaptive rather than a hard limit.
    pub fn new(event_limit_target: usize, max_requests: usize) -> Self {
        assert!(event_limit_target > 0);
        assert!(max_requests > 0);

        let initial_permits = MINIMUM_PERMITS.min(max_requests);
        let semaphore = Arc::new(Semaphore::new(initial_permits));
        Self {
            semaphore: Arc::clone(&semaphore),
            data: Arc::new(Mutex::new(RequestLimiterData {
                event_limit_target,
                minimum_permits: initial_permits,
                total_permits: initial_permits,
                average_request_size: EwmaDefault::new(EWMA_WEIGHT, event_limit_target as f64),
                semaphore,
                max_requests,
            })),
        }
    }

    pub async fn acquire(&self) -> RequestLimiterPermit {
        let permit = Arc::clone(&self.semaphore)
            .acquire_owned()
            .await
            .expect("request limiter semaphore must remain open");
        self.build_permit(permit)
    }

    pub fn try_acquire(&self) -> Option<RequestLimiterPermit> {
        Arc::clone(&self.semaphore)
            .try_acquire_owned()
            .ok()
            .map(|permit| self.build_permit(permit))
    }

    fn build_permit(&self, semaphore_permit: OwnedSemaphorePermit) -> RequestLimiterPermit {
        RequestLimiterPermit {
            semaphore_permit: Some(semaphore_permit),
            request_limiter_data: Arc::clone(&self.data),
        }
    }
}

#[cfg(test)]
mod test {
    use approx::assert_abs_diff_eq;

    use super::*;

    #[tokio::test]
    async fn test_average_convergence() {
        let limiter = RequestLimiter::new(100, 100);

        for _ in 0..100 {
            let permit = limiter.acquire().await;
            permit.decoding_finished(5);
            drop(permit);
        }
        let data = limiter.data.lock().unwrap();
        assert_abs_diff_eq!(data.target_requests_in_flight(), 100 / 5, epsilon = 1);
    }

    #[tokio::test]
    async fn test_minimum_permits() {
        let limiter = RequestLimiter::new(100, 100);

        for _ in 0..100 {
            let permit = limiter.acquire().await;
            permit.decoding_finished(500);
            drop(permit);
        }
        let data = limiter.data.lock().unwrap();
        assert_eq!(data.target_requests_in_flight(), MINIMUM_PERMITS);
    }

    #[tokio::test]
    async fn test_maximum_permits() {
        let request_limit = 50;
        let limiter = RequestLimiter::new(1000, request_limit);

        for _ in 0..100 {
            let permit = limiter.acquire().await;
            permit.decoding_finished(1);
            drop(permit);
        }
        let data = limiter.data.lock().unwrap();
        assert_eq!(data.target_requests_in_flight(), request_limit);
    }

    #[test]
    fn try_acquire_is_non_blocking_and_adaptive() {
        let limiter = RequestLimiter::new(100, 10);
        let first = limiter.try_acquire().unwrap();
        let second = limiter.try_acquire().unwrap();
        assert!(limiter.try_acquire().is_none());

        first.decoding_finished(1);
        drop(first);
        assert!(limiter.try_acquire().is_some());

        drop(second);
    }

    #[test]
    fn configured_maximum_below_default_initial_permits_is_respected() {
        let limiter = RequestLimiter::new(100, 1);
        let permit = limiter.try_acquire().unwrap();
        assert!(limiter.try_acquire().is_none());
        drop(permit);
        assert!(limiter.try_acquire().is_some());
    }
}

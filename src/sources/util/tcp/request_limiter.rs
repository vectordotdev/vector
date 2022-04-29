use crate::stats::EwmaDefault;
use std::cmp::Ordering;
use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EWMA_WEIGHT: f64 = 0.1;
const MINIMUM_PERMITS: usize = 2;

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
                    // only release the current permit (when the inner permit is dropped automatically)
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
    total_permits: usize,
    average_request_size: EwmaDefault,
    semaphore: Arc<Semaphore>,
    max_requests: usize,
}

impl RequestLimiterData {
    pub fn update_average(&mut self, num_events: usize) {
        if num_events > 0 {
            self.average_request_size.update(num_events as f64);
        }
    }

    pub fn target_requests_in_flight(&self) -> usize {
        let target = (self.event_limit_target as f64) / self.average_request_size.average();
        (target as usize)
            .max(MINIMUM_PERMITS)
            .min(self.max_requests)
    }

    pub fn increase_permits(&mut self) {
        self.total_permits += 1;
        self.semaphore.add_permits(1);
    }

    pub fn decrease_permits(&mut self, permit: OwnedSemaphorePermit) {
        if self.total_permits > MINIMUM_PERMITS {
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
    /// event_limit_target: The limit to the number of events that will be in-flight at one time.
    /// max_requests: The most number of requests that can be processed concurrently
    /// The numbers of events in a request is not known until after it has been decoded, so this is not a hard limit.
    pub fn new(event_limit_target: usize, max_requests: usize) -> RequestLimiter {
        assert!(event_limit_target > 0);

        let semaphore = Arc::new(Semaphore::new(MINIMUM_PERMITS));
        RequestLimiter {
            semaphore: Arc::clone(&semaphore),
            data: Arc::new(Mutex::new(RequestLimiterData {
                event_limit_target,
                total_permits: MINIMUM_PERMITS,
                average_request_size: EwmaDefault::new(EWMA_WEIGHT, event_limit_target as f64),
                semaphore,
                max_requests,
            })),
        }
    }

    pub async fn acquire(&self) -> RequestLimiterPermit {
        let permit = Arc::clone(&self.semaphore).acquire_owned().await;
        RequestLimiterPermit {
            semaphore_permit: permit.ok(),
            request_limiter_data: Arc::clone(&self.data),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use approx::assert_abs_diff_eq;

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
}

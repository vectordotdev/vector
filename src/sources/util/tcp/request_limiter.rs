use std::sync::{Arc, Mutex};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const EWMA_WEIGHT: f32 = 0.1;
const INVERSE_EWMA_WEIGHT: f32 = 1.0 - EWMA_WEIGHT;

pub struct RequestLimiterPermit {
    semaphore_permit: Option<OwnedSemaphorePermit>,
    request_limiter_data: Arc<Mutex<RequestLimiterData>>,
    num_events: usize,
}

impl RequestLimiterPermit {
    pub fn decoding_finished(&mut self, num_events: usize) {
        self.num_events = num_events;
        let mut request_limiter_data = self.request_limiter_data.lock().unwrap();
        // request_limiter_data.current_in_flight += num_events;
        request_limiter_data.update_average(num_events);
        info!(
            "Request of size {} changed average to {}",
            num_events, request_limiter_data.average_request_size
        );

        //TODO: potential optimization: decide if the permit can be released early here
    }
}

impl Drop for RequestLimiterPermit {
    fn drop(&mut self) {
        if let Ok(mut request_limiter_data) = self.request_limiter_data.lock() {
            let target = request_limiter_data.target_requests_in_flight();
            let current = request_limiter_data.total_permits;

            if target > current {
                request_limiter_data.increase_permits();
            } else if target == current {
                // only release the current permit (when the inner permit is dropped automatically)
            } else {
                // target < current
                let permit = self.semaphore_permit.take().unwrap();
                request_limiter_data.decrease_permits(permit);
            }
        }
    }
}

struct RequestLimiterData {
    event_limit_target: usize,
    total_permits: usize,
    average_request_size: f32,
    semaphore: Arc<Semaphore>,
    num_cpus: usize,
}

impl RequestLimiterData {
    pub fn update_average(&mut self, num_events: usize) {
        let num_events = num_events as f32;
        self.average_request_size =
            (EWMA_WEIGHT * num_events) + (INVERSE_EWMA_WEIGHT * self.average_request_size);
    }

    pub fn target_requests_in_flight(&self) -> usize {
        let target = ((self.event_limit_target as f32) / self.average_request_size) as usize;
        target.min(self.num_cpus).max(1)
    }

    pub fn increase_permits(&mut self) {
        self.total_permits += 1;
        self.semaphore.add_permits(1);
    }

    pub fn decrease_permits(&mut self, permit: OwnedSemaphorePermit) {
        if self.total_permits > 1 {
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
    /// The numbers of events in a request is not known until after it has been decoded, so this is not a hard limit.
    pub fn new(event_limit_target: usize) -> RequestLimiter {
        let initial_permits = 1;
        let semaphore = Arc::new(Semaphore::new(initial_permits));
        RequestLimiter {
            semaphore: semaphore.clone(),
            data: Arc::new(Mutex::new(RequestLimiterData {
                event_limit_target,
                total_permits: initial_permits,

                // average is initialized to the target so that target requests starts at 1
                average_request_size: event_limit_target as f32,
                semaphore,
                num_cpus: num_cpus::get(),
            })),
        }
    }

    pub async fn acquire(&self) -> RequestLimiterPermit {
        // The semaphore is never closed, so this cannot fail
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        RequestLimiterPermit {
            semaphore_permit: Some(permit),
            request_limiter_data: self.data.clone(),
            num_events: 0,
        }
    }
}

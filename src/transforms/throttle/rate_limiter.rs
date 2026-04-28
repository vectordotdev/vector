use std::{hash::Hash, sync::Arc, time::Duration};

use governor::{
    Quota, RateLimiter, clock, middleware::NoOpMiddleware, state::keyed::DashMapStateStore,
};
use metrics::Counter;
use tokio;

use crate::cpu_time::CpuTimedExt;

/// Re-usable wrapper around the structs/type from the governor crate.
/// Spawns a background task that periodically flushes keys that haven't been accessed recently.
pub struct RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone,
    C: clock::Clock,
{
    pub rate_limiter: Arc<RateLimiter<K, DashMapStateStore<K>, C, NoOpMiddleware<C::Instant>>>,
    flush_handle: tokio::task::JoinHandle<()>,
}

impl<K, C> RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    C: clock::Clock + Clone + Send + Sync + 'static,
{
    pub fn start(quota: Quota, clock: C, flush_keys_interval: Duration, cpu_ns: Counter) -> Self {
        let rate_limiter = Arc::new(RateLimiter::dashmap_with_clock(quota, clock));

        let rate_limiter_clone = Arc::clone(&rate_limiter);
        // Hook the periodic key-flush task onto the component's CPU counter so
        // its housekeeping work is attributed to this throttle transform.
        let flush_handle = tokio::spawn(
            async move {
                let mut interval = tokio::time::interval(flush_keys_interval);
                loop {
                    interval.tick().await;
                    rate_limiter_clone.retain_recent();
                }
            }
            .cpu_timed(cpu_ns),
        );

        Self {
            rate_limiter,
            flush_handle,
        }
    }

    pub fn check_key(&self, key: &K) -> bool {
        self.rate_limiter.check_key(key).is_ok()
    }
}

impl<K, C> Drop for RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone,
    C: clock::Clock,
{
    fn drop(&mut self) {
        self.flush_handle.abort();
    }
}

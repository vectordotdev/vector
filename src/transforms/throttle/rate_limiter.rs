use governor::clock;
use governor::middleware::NoOpMiddleware;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio;

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
    pub fn start(quota: Quota, clock: C, flush_keys_interval: Duration) -> Self {
        let rate_limiter = Arc::new(RateLimiter::dashmap_with_clock(quota, clock));

        let rate_limiter_clone = Arc::clone(&rate_limiter);
        let flush_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(flush_keys_interval);
            loop {
                interval.tick().await;
                rate_limiter_clone.retain_recent();
            }
        });

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

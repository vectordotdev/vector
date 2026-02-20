use std::{hash::Hash, num::NonZeroU32, sync::Arc, time::Duration};

use governor::{
    Quota, RateLimiter, clock, middleware::NoOpMiddleware, state::keyed::DashMapStateStore,
};
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

    /// Check whether `n` tokens can be consumed for the given key.
    ///
    /// Returns `true` if the tokens were consumed (within rate limit).
    /// Returns `false` if the rate limit would be exceeded.
    /// If `n` exceeds the bucket's total burst capacity (`InsufficientCapacity`),
    /// returns `true` and allows the event through — permanently rejecting events
    /// larger than the burst size would be incorrect behavior.
    pub fn check_key_n(&self, key: &K, n: NonZeroU32) -> bool {
        match self.rate_limiter.check_key_n(key, n) {
            Ok(ok) => ok.is_ok(),
            // InsufficientCapacity: n > burst size. Allow through rather than
            // permanently blocking events that exceed the burst capacity.
            Err(_) => true,
        }
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

use std::{hash::Hash, num::NonZeroU32, sync::Arc, time::Duration};

use governor::{
    Quota, RateLimiter, clock, middleware::NoOpMiddleware,
    state::InMemoryState,
    state::keyed::DashMapStateStore,
};
use tokio;

/// Re-usable wrapper around the structs/type from the governor crate.
/// Spawns a background task that periodically flushes keys that haven't been accessed recently.
pub struct KeyedRateLimiter<K, C>
where
    K: Hash + Eq + Clone,
    C: clock::Clock,
{
    pub rate_limiter: Arc<RateLimiter<K, DashMapStateStore<K>, C, NoOpMiddleware<C::Instant>>>,
    flush_handle: tokio::task::JoinHandle<()>,
}

impl<K, C> KeyedRateLimiter<K, C>
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
    /// returns `false` — the event is throttled like any other over-budget event.
    /// This prevents oversized events from bypassing rate limiting entirely.
    pub fn check_key_n(&self, key: &K, n: NonZeroU32) -> bool {
        match self.rate_limiter.check_key_n(key, n) {
            Ok(ok) => ok.is_ok(),
            // InsufficientCapacity: n > burst size. Throttle the event.
            Err(_) => {
                warn!(
                    message = "Event cost exceeds burst capacity, throttling. Consider increasing threshold.",
                    cost = n.get(),
                    internal_log_rate_secs = 10,
                );
                false
            }
        }
    }
}

impl<K, C> Drop for KeyedRateLimiter<K, C>
where
    K: Hash + Eq + Clone,
    C: clock::Clock,
{
    fn drop(&mut self) {
        self.flush_handle.abort();
    }
}

/// Lightweight unkeyed rate limiter for when `key_field` is not configured.
/// Uses a single in-memory bucket instead of DashMap, avoiding per-key hashing,
/// concurrent map overhead, and the background `retain_recent` task.
pub struct DirectRateLimiter<C: clock::Clock> {
    rate_limiter: RateLimiter<governor::state::NotKeyed, InMemoryState, C, NoOpMiddleware<C::Instant>>,
}

impl<C> DirectRateLimiter<C>
where
    C: clock::Clock,
{
    pub fn new(quota: Quota, clock: C) -> Self {
        Self {
            rate_limiter: RateLimiter::direct_with_clock(quota, clock),
        }
    }

    pub fn check(&self) -> bool {
        self.rate_limiter.check().is_ok()
    }

    pub fn check_n(&self, n: NonZeroU32) -> bool {
        match self.rate_limiter.check_n(n) {
            Ok(ok) => ok.is_ok(),
            Err(_) => {
                warn!(
                    message = "Event cost exceeds burst capacity, throttling. Consider increasing threshold.",
                    cost = n.get(),
                    internal_log_rate_secs = 10,
                );
                false
            }
        }
    }
}

/// Unified rate limiter that dispatches to either a keyed (DashMap) or
/// direct (single-bucket) implementation based on whether `key_field` is configured.
pub enum RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone,
    C: clock::Clock,
{
    Keyed(KeyedRateLimiter<K, C>),
    Direct(DirectRateLimiter<C>),
}

impl<K, C> RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    C: clock::Clock + Clone + Send + Sync + 'static,
{
    pub fn start_keyed(quota: Quota, clock: C, flush_keys_interval: Duration) -> Self {
        Self::Keyed(KeyedRateLimiter::start(quota, clock, flush_keys_interval))
    }

    pub fn start_direct(quota: Quota, clock: C) -> Self {
        Self::Direct(DirectRateLimiter::new(quota, clock))
    }

    pub fn check_key(&self, key: &K) -> bool {
        match self {
            Self::Keyed(k) => k.check_key(key),
            Self::Direct(d) => d.check(),
        }
    }

    pub fn check_key_n(&self, key: &K, n: NonZeroU32) -> bool {
        match self {
            Self::Keyed(k) => k.check_key_n(key, n),
            Self::Direct(d) => d.check_n(n),
        }
    }
}

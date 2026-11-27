use std::{hash::Hash, num::NonZeroU32, sync::Arc, time::Duration};

use dashmap::DashMap;
use governor::{
    Quota, RateLimiter, clock, clock::Reference as _, middleware::NoOpMiddleware,
    state::keyed::DashMapStateStore,
};
use vector_lib::cpu_time::spawn_timed;

type FixedLimiter<K, C> =
    RateLimiter<K, DashMapStateStore<K>, C, NoOpMiddleware<<C as clock::Clock>::Instant>>;

struct DynamicLimiter<K, C: clock::Clock> {
    replenished_at: DashMap<K, u128>,
    clock: C,
    start: C::Instant,
    window_nanos: u128,
}

// Rounding down each cost across a burst of u32::MAX events loses fewer units
// than the cost of a single event, even for a one-nanosecond window.
const UNITS_PER_WINDOW: u64 = u64::MAX;

impl<K: Hash + Eq + Clone, C: clock::Clock> DynamicLimiter<K, C> {
    fn new(window: Duration, clock: C) -> Self {
        Self {
            replenished_at: DashMap::new(),
            start: clock.now(),
            clock,
            window_nanos: window.as_nanos(),
        }
    }

    fn now(&self) -> u128 {
        let elapsed = self.clock.now().duration_since(self.start).as_u64();
        u128::from(elapsed) * u128::from(UNITS_PER_WINDOW) / self.window_nanos
    }

    fn check_key(&self, key: &K, threshold: NonZeroU32) -> bool {
        let cost = UNITS_PER_WINDOW / u64::from(threshold.get());
        let mut replenished_at = self
            .replenished_at
            .get_mut(key)
            .unwrap_or_else(|| self.replenished_at.entry(key.clone()).or_default());
        let now = self.now();

        // Each accepted event pushes back the time when the bucket will be full again.
        // Across all thresholds, that time can be at most one window ahead of the clock.
        let next = (*replenished_at).max(now) + u128::from(cost);
        if next - now > u128::from(UNITS_PER_WINDOW) {
            return false;
        }
        *replenished_at = next;
        true
    }

    fn retain_recent(&self) {
        let now = self.now();
        self.replenished_at.retain(|_, deadline| *deadline > now);
    }
}

enum KeyedLimiter<K: Hash + Eq + Clone, C: clock::Clock> {
    Fixed(FixedLimiter<K, C>),
    Dynamic(DynamicLimiter<K, C>),
}

/// A keyed rate limiter with periodic removal of replenished buckets.
pub struct RateLimiterRunner<K: Hash + Eq + Clone, C: clock::Clock> {
    limiter: Arc<KeyedLimiter<K, C>>,
    flush_handle: tokio::task::JoinHandle<()>,
}

impl<K, C> RateLimiterRunner<K, C>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    C: clock::Clock + Send + Sync + 'static,
{
    pub fn start(static_quota: Option<Quota>, window: Duration, clock: C) -> Self {
        let limiter = Arc::new(match static_quota {
            Some(quota) => KeyedLimiter::Fixed(RateLimiter::dashmap_with_clock(quota, clock)),
            None => KeyedLimiter::Dynamic(DynamicLimiter::new(window, clock)),
        });
        let flush_limiter = Arc::clone(&limiter);
        let flush_handle = spawn_timed(async move {
            let mut interval = tokio::time::interval(window);
            loop {
                interval.tick().await;
                match flush_limiter.as_ref() {
                    KeyedLimiter::Fixed(limiter) => limiter.retain_recent(),
                    KeyedLimiter::Dynamic(limiter) => limiter.retain_recent(),
                }
            }
        });
        Self {
            limiter,
            flush_handle,
        }
    }

    pub fn check_key(&self, key: &K, threshold: NonZeroU32) -> bool {
        match self.limiter.as_ref() {
            KeyedLimiter::Fixed(limiter) => limiter.check_key(key).is_ok(),
            KeyedLimiter::Dynamic(limiter) => limiter.check_key(key, threshold),
        }
    }
}

impl<K: Hash + Eq + Clone, C: clock::Clock> Drop for RateLimiterRunner<K, C> {
    fn drop(&mut self) {
        self.flush_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_thresholds_fill_one_window() {
        let clock = clock::FakeRelativeClock::default();
        let limiter = DynamicLimiter::new(Duration::from_secs(1), clock.clone());

        for threshold in [6, 6, 3, 3] {
            assert!(limiter.check_key(&"thirds", NonZeroU32::new(threshold).unwrap()));
        }
        assert!(!limiter.check_key(&"thirds", NonZeroU32::MAX));

        assert!(limiter.check_key(&"full", NonZeroU32::MIN));
        assert!(!limiter.check_key(&"full", NonZeroU32::new(600_000).unwrap()));
        clock.advance(Duration::from_millis(100));
        assert!(limiter.check_key(&"full", NonZeroU32::new(750_000_000).unwrap()));
    }

    #[test]
    fn full_threshold_range_in_one_nanosecond() {
        let clock = clock::FakeRelativeClock::default();
        let limiter = DynamicLimiter::new(Duration::from_nanos(1), clock.clone());
        assert!(limiter.check_key(&"bucket", NonZeroU32::MAX));
        assert!(limiter.check_key(&"bucket", NonZeroU32::MAX));
        assert!(!limiter.check_key(&"bucket", NonZeroU32::MIN));
        clock.advance(Duration::from_nanos(1));
        assert!(limiter.check_key(&"bucket", NonZeroU32::MIN));
        assert!(!limiter.check_key(&"bucket", NonZeroU32::MAX));
    }

    #[tokio::test(start_paused = true)]
    async fn removes_only_replenished_buckets() {
        let clock = clock::FakeRelativeClock::default();
        let window = Duration::from_secs(2);
        let runner = RateLimiterRunner::start(None, window, clock.clone());
        assert!(runner.check_key(&"full", NonZeroU32::MIN));
        assert!(runner.check_key(&"half", NonZeroU32::new(2).unwrap()));
        tokio::task::yield_now().await;
        clock.advance(Duration::from_secs(1));
        tokio::time::advance(window).await;
        tokio::task::yield_now().await;

        let KeyedLimiter::Dynamic(limiter) = runner.limiter.as_ref() else {
            unreachable!()
        };
        assert!(!limiter.replenished_at.contains_key(&"half"));
        assert!(limiter.replenished_at.contains_key(&"full"));
        assert!(!runner.check_key(&"full", NonZeroU32::MIN));
    }
}

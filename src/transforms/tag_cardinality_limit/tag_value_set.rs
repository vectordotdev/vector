//! Storage backends for accepted tag values.
//!
//! Four variants, picked at construction time from `(Mode, ttl_secs)`:
//!
//! - `Set` — `HashSet`, no TTL. Original exact-mode behavior.
//! - `Bloom` — single `BloomFilter`, no TTL. Original probabilistic-mode behavior.
//! - `TtlSet` — `HashMap<value, last_seen>` with periodic sweep. Exact mode + TTL.
//! - `RollingBloom` — `VecDeque<BloomFilter>` of `ttl_generations` shards, lazily
//!   rotated. Probabilistic mode + TTL.
//!
//! Both TTL variants use "refresh on sighting" semantics: every `contains()` hit
//! extends the value's lease, so continuously-observed values stay in the cache
//! across rotation boundaries. Eviction is lazy — driven by `insert()` and
//! `contains()` calls — so there's no background task.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    time::{Duration, Instant},
};

use bloomy::BloomFilter;

use crate::{
    event::metric::TagValueSet,
    internal_events::TagCardinalityTtlExpired,
    transforms::tag_cardinality_limit::config::Mode,
};

/// Container for storing the set of accepted values for a given tag key.
#[derive(Debug)]
pub struct AcceptedTagValueSet {
    storage: TagValueSetStorage,
}

enum TagValueSetStorage {
    Set(HashSet<TagValueSet>),
    Bloom(BloomFilterStorage),
    TtlSet(TtlExactStorage),
    RollingBloom(RollingBloomStorage),
}

/// A bloom filter that tracks the number of items inserted into it.
struct BloomFilterStorage {
    inner: BloomFilter<TagValueSet>,

    /// Count of items inserted into the bloom filter.
    /// We manually track this because `BloomFilter::count` has O(n) time complexity.
    count: usize,
}

impl BloomFilterStorage {
    fn new(size: usize) -> Self {
        Self {
            inner: BloomFilter::with_size(size),
            count: 0,
        }
    }

    fn insert(&mut self, value: &TagValueSet) {
        // Only update the count if the value is not already in the bloom filter.
        if !self.inner.contains(value) {
            self.inner.insert(value);
            self.count += 1;
        }
    }

    fn contains(&self, value: &TagValueSet) -> bool {
        self.inner.contains(value)
    }

    const fn count(&self) -> usize {
        self.count
    }
}

// =============================================================================
// Exact mode + TTL
// =============================================================================

/// `HashMap`-backed exact cache with per-value last-seen timestamps.
///
/// Sweep policy: at most once per `sweep_interval` (= `ttl / max(generations, 1)`,
/// floored to 1 second), `retain` drops every entry whose `last_seen` is older
/// than `ttl`. The sweep runs inside `insert`/`contains`/`len` — lazy, no
/// background task.
struct TtlExactStorage {
    map: HashMap<TagValueSet, Instant>,
    ttl: Duration,
    sweep_interval: Duration,
    last_sweep: Instant,
}

impl TtlExactStorage {
    fn new(value_limit: usize, ttl: Duration, generations: u8) -> Self {
        let now = Instant::now();
        let divisor = generations.max(1) as u32;
        let sweep_interval = (ttl / divisor).max(Duration::from_secs(1));
        Self {
            map: HashMap::with_capacity(value_limit),
            ttl,
            sweep_interval,
            last_sweep: now,
        }
    }

    fn maybe_sweep(&mut self, now: Instant) {
        if now.duration_since(self.last_sweep) < self.sweep_interval {
            return;
        }
        self.sweep(now);
    }

    fn sweep(&mut self, now: Instant) {
        let ttl = self.ttl;
        let before = self.map.len();
        self.map
            .retain(|_, last_seen| now.duration_since(*last_seen) <= ttl);
        let expired = before.saturating_sub(self.map.len()) as u64;
        self.last_sweep = now;
        emit!(TagCardinalityTtlExpired { count: expired });
    }

    fn contains(&mut self, value: &TagValueSet) -> bool {
        let now = Instant::now();
        self.maybe_sweep(now);
        // Refresh lease on every sighting so continuously-seen values don't blink out.
        if let Some(slot) = self.map.get_mut(value) {
            *slot = now;
            true
        } else {
            false
        }
    }

    /// Read-only membership check: triggers lazy sweep so the answer reflects
    /// post-expiry state, but does **not** refresh the value's lease. Used in
    /// `DropEvent` pre-check paths where we must not mutate cache state for
    /// events that are about to be dropped.
    fn contains_no_refresh(&mut self, value: &TagValueSet) -> bool {
        let now = Instant::now();
        self.maybe_sweep(now);
        self.map.contains_key(value)
    }

    fn insert(&mut self, value: TagValueSet) {
        let now = Instant::now();
        self.maybe_sweep(now);
        self.map.insert(value, now);
    }

    fn len(&mut self) -> usize {
        let now = Instant::now();
        self.maybe_sweep(now);
        self.map.len()
    }
}

// =============================================================================
// Probabilistic mode + TTL: rolling bloom filter
// =============================================================================

/// Sliding-window bloom: `generations` shards, each a full `cache_size_per_key`
/// bloom filter. Front of the deque is the oldest shard; back is the current.
/// On rotation, the front shard is dropped and a fresh empty one is pushed at the
/// back. Membership is the OR across shards; refresh-on-sighting writes hits
/// into the current shard so hot values survive future rotations.
struct RollingBloomStorage {
    shards: VecDeque<BloomFilterStorage>,
    generations: u8,
    slice: Duration,
    cache_size_per_key: usize,
    /// The boundary at which the next rotation is due. Advances by `slice` on
    /// every rotation. Storing the *next-tick* timestamp instead of "last_rotate"
    /// makes the catch-up loop in `rotate_if_needed` trivial and tolerant of
    /// long pauses between calls.
    next_rotate: Instant,
}

impl RollingBloomStorage {
    fn new(cache_size_per_key: usize, generations: u8, ttl: Duration) -> Self {
        let generations = generations.max(1);
        // Avoid a zero-duration slice (would cause `rotate_if_needed` to spin).
        let slice = (ttl / generations as u32).max(Duration::from_secs(1));
        let mut shards = VecDeque::with_capacity(generations as usize);
        shards.push_back(BloomFilterStorage::new(cache_size_per_key));
        let now = Instant::now();
        Self {
            shards,
            generations,
            slice,
            cache_size_per_key,
            next_rotate: now + slice,
        }
    }

    fn rotate_if_needed(&mut self, now: Instant) {
        // Catch up if we've been idle for multiple slices. Capped to `generations`
        // pops because every shard would have rotated out anyway.
        let mut rotations = 0u8;
        while now >= self.next_rotate && rotations < self.generations {
            if self.shards.len() >= self.generations as usize
                && let Some(dropped) = self.shards.pop_front()
            {
                emit!(TagCardinalityTtlExpired {
                    count: dropped.count() as u64,
                });
            }
            self.shards
                .push_back(BloomFilterStorage::new(self.cache_size_per_key));
            self.next_rotate += self.slice;
            rotations += 1;
        }
        // If we needed more rotations than `generations`, the whole window is
        // stale — fast-forward `next_rotate` to avoid a tight catch-up the next
        // call after a long idle period.
        if now >= self.next_rotate {
            self.next_rotate = now + self.slice;
        }
    }

    fn contains(&mut self, value: &TagValueSet) -> bool {
        let now = Instant::now();
        self.rotate_if_needed(now);

        // Check newest -> oldest so hot values short-circuit immediately.
        let found = self.shards.iter().rev().any(|s| s.contains(value));
        if found {
            // Refresh: ensure the value is in the current shard so it survives
            // the next rotation. `BloomFilterStorage::insert` is idempotent.
            if let Some(newest) = self.shards.back_mut() {
                newest.insert(value);
            }
        }
        found
    }

    /// Read-only membership check: triggers lazy rotation but does **not**
    /// refresh the value's presence in the current shard. See
    /// `TtlExactStorage::contains_no_refresh` for the rationale.
    fn contains_no_refresh(&mut self, value: &TagValueSet) -> bool {
        let now = Instant::now();
        self.rotate_if_needed(now);
        self.shards.iter().rev().any(|s| s.contains(value))
    }

    fn insert(&mut self, value: &TagValueSet) {
        let now = Instant::now();
        self.rotate_if_needed(now);
        if let Some(newest) = self.shards.back_mut() {
            newest.insert(value);
        }
    }

    fn len(&mut self) -> usize {
        let now = Instant::now();
        self.rotate_if_needed(now);
        // Cardinality is bounded above by any individual shard's count under
        // refresh-on-sighting (hot values are present in every shard). Taking
        // the max is cheap and converges to the true unique count as soon as
        // every retained value has been seen at least once per slice.
        self.shards.iter().map(|s| s.count()).max().unwrap_or(0)
    }
}

// =============================================================================

impl fmt::Debug for TagValueSetStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TagValueSetStorage::Set(set) => write!(f, "Set({set:?})"),
            TagValueSetStorage::Bloom(_) => write!(f, "Bloom"),
            TagValueSetStorage::TtlSet(s) => {
                write!(f, "TtlSet(len={}, ttl={:?})", s.map.len(), s.ttl)
            }
            TagValueSetStorage::RollingBloom(s) => {
                write!(
                    f,
                    "RollingBloom(generations={}, slice={:?})",
                    s.generations, s.slice
                )
            }
        }
    }
}

impl AcceptedTagValueSet {
    /// Construct the appropriate backend.
    ///
    /// When `ttl_secs` is `None` (default), the backend is identical to the pre-TTL
    /// behavior — `HashSet` for exact, single `BloomFilter` for probabilistic — so
    /// existing configs see zero behavioral change.
    pub fn new(
        value_limit: usize,
        mode: &Mode,
        ttl_secs: Option<u64>,
        ttl_generations: u8,
    ) -> Self {
        let ttl = ttl_secs.and_then(|s| (s > 0).then(|| Duration::from_secs(s)));

        let storage = match (mode, ttl) {
            (Mode::Exact, None) => TagValueSetStorage::Set(HashSet::with_capacity(value_limit)),
            (Mode::Exact, Some(ttl)) => {
                TagValueSetStorage::TtlSet(TtlExactStorage::new(value_limit, ttl, ttl_generations))
            }
            (Mode::Probabilistic(config), None) => {
                TagValueSetStorage::Bloom(BloomFilterStorage::new(config.cache_size_per_key))
            }
            (Mode::Probabilistic(config), Some(ttl)) => TagValueSetStorage::RollingBloom(
                RollingBloomStorage::new(config.cache_size_per_key, ttl_generations, ttl),
            ),
        };
        Self { storage }
    }

    /// Returns true if `value` is currently retained.
    ///
    /// In TTL-enabled backends this is a mutating operation: it triggers lazy
    /// sweep/rotation **and refreshes the value's lease on a hit**. Use this
    /// on the accept path (`try_accept_tag`, where a hit means we keep the
    /// value). For read-only checks where the event might still be rejected,
    /// use [`Self::contains_no_refresh`].
    pub fn contains(&mut self, value: &TagValueSet) -> bool {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => set.contains(value),
            TagValueSetStorage::Bloom(bloom) => bloom.contains(value),
            TagValueSetStorage::TtlSet(s) => s.contains(value),
            TagValueSetStorage::RollingBloom(s) => s.contains(value),
        }
    }

    /// Like [`Self::contains`] but never refreshes the value's TTL lease.
    ///
    /// The `DropEvent` pre-check pass uses this so that an event rejected by a
    /// later tag does not silently extend the leases of earlier-checked values.
    /// The semantic of TTL eviction is "what's been *accepted* in the last N
    /// seconds", not "what's been *seen* in the last N seconds".
    pub fn contains_no_refresh(&mut self, value: &TagValueSet) -> bool {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => set.contains(value),
            TagValueSetStorage::Bloom(bloom) => bloom.contains(value),
            TagValueSetStorage::TtlSet(s) => s.contains_no_refresh(value),
            TagValueSetStorage::RollingBloom(s) => s.contains_no_refresh(value),
        }
    }

    /// Number of distinct values currently retained.
    ///
    /// In TTL-enabled backends this also triggers lazy sweep/rotation so the
    /// returned figure reflects post-expiry state.
    pub fn len(&mut self) -> usize {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => set.len(),
            TagValueSetStorage::Bloom(bloom) => bloom.count(),
            TagValueSetStorage::TtlSet(s) => s.len(),
            TagValueSetStorage::RollingBloom(s) => s.len(),
        }
    }

    pub fn insert(&mut self, value: TagValueSet) {
        match &mut self.storage {
            TagValueSetStorage::Set(set) => {
                set.insert(value);
            }
            TagValueSetStorage::Bloom(bloom) => bloom.insert(&value),
            TagValueSetStorage::TtlSet(s) => s.insert(value),
            TagValueSetStorage::RollingBloom(s) => s.insert(&value),
        };
    }

    /// Test-only accessor: true iff this set uses a TTL-enabled backend.
    /// Lets tests pin backend selection without exposing the internal enum.
    #[cfg(test)]
    pub(crate) fn ttl_enabled(&self) -> bool {
        matches!(
            self.storage,
            TagValueSetStorage::TtlSet(_) | TagValueSetStorage::RollingBloom(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::metric::TagValueSet,
        transforms::tag_cardinality_limit::config::{BloomFilterConfig, Mode, default_cache_size},
    };

    fn v(s: &str) -> TagValueSet {
        TagValueSet::from([s.to_string()])
    }

    #[test]
    fn exact_no_ttl_preserves_today_behavior() {
        let mut set = AcceptedTagValueSet::new(2, &Mode::Exact, None, 4);
        assert!(!set.contains(&v("a")));
        assert_eq!(set.len(), 0);
        set.insert(v("a"));
        set.insert(v("b"));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&v("a")));
        assert!(set.contains(&v("b")));
    }

    #[test]
    fn bloom_no_ttl_preserves_today_behavior() {
        let mode = Mode::Probabilistic(BloomFilterConfig {
            cache_size_per_key: default_cache_size(),
        });
        let mut set = AcceptedTagValueSet::new(2, &mode, None, 4);
        set.insert(v("a"));
        set.insert(v("a"));
        assert_eq!(set.len(), 1, "duplicate insert must not bump count");
        set.insert(v("b"));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&v("a")));
        assert!(set.contains(&v("b")));
    }

    // -------------------- TtlExactStorage --------------------
    //
    // We exercise the storage type directly so we can advance wall-clock time
    // via the `now: Instant` parameter on private helpers. `AcceptedTagValueSet`
    // itself uses `Instant::now()`, which can't be mocked cheaply.

    #[test]
    fn ttl_exact_expires_values_past_ttl() {
        let ttl = Duration::from_secs(60);
        let mut s = TtlExactStorage::new(8, ttl, 4);
        // t0: insert v=a manually so we control its timestamp.
        let t0 = Instant::now();
        s.map.insert(v("a"), t0);
        s.last_sweep = t0;
        // t0+30s: still alive.
        s.sweep(t0 + Duration::from_secs(30));
        assert!(s.map.contains_key(&v("a")));
        // t0+90s: expired.
        s.sweep(t0 + Duration::from_secs(90));
        assert!(!s.map.contains_key(&v("a")));
    }

    #[test]
    fn ttl_exact_refresh_on_contains_extends_lease() {
        // Seed the map with an old timestamp, then drive a real `contains`.
        // The refresh path (`*slot = now;`) must push the stored Instant
        // forward — otherwise sweeps continue to use the old (potentially
        // expired) timestamp. A short sleep guarantees `Instant::now()` is
        // strictly after `t_insert` on every platform.
        let mut s = TtlExactStorage::new(8, Duration::from_secs(60), 4);
        let t_insert = Instant::now();
        s.map.insert(v("a"), t_insert);
        s.last_sweep = t_insert;

        std::thread::sleep(Duration::from_millis(2));

        assert!(s.contains(&v("a")));
        let after = *s.map.get(&v("a")).expect("entry should still be present");
        assert!(
            after > t_insert,
            "contains() must refresh the stored Instant; was {t_insert:?}, still {after:?}"
        );
    }

    #[test]
    fn ttl_exact_sweep_interval_floors_to_one_second() {
        // ttl=2s, generations=8 → naive slice = 250ms; we floor to 1s so sweeps
        // never become dominant. Verify the floor.
        let s = TtlExactStorage::new(8, Duration::from_secs(2), 8);
        assert!(s.sweep_interval >= Duration::from_secs(1));
    }

    #[test]
    fn ttl_exact_contains_no_refresh_does_not_extend_lease() {
        // Regression for the `DropEvent` pre-check bug: a read-only check
        // must NOT update the stored Instant.
        let ttl = Duration::from_secs(60);
        let mut s = TtlExactStorage::new(8, ttl, 4);
        let t0 = Instant::now();
        s.map.insert(v("a"), t0);
        s.last_sweep = t0;
        assert!(s.contains_no_refresh(&v("a")));
        assert!(s.contains_no_refresh(&v("a")));
        assert_eq!(
            s.map.get(&v("a")).copied(),
            Some(t0),
            "timestamp must remain at t0 after no-refresh checks"
        );
        // Sanity: the refreshing variant `contains` *does* update the
        // timestamp. We can't pin its exact post-call value (it depends on
        // `Instant::now()`), but it must have moved forward.
        s.contains(&v("a"));
        assert!(s.map.get(&v("a")).copied().unwrap() >= t0);
    }

    #[test]
    fn rolling_bloom_contains_no_refresh_does_not_seed_newest_shard() {
        // Same regression as above, but for the probabilistic backend: a
        // no-refresh check must not insert into the newest shard.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        s.shards.back_mut().unwrap().insert(&v("a"));
        // Drive a rotation so we have a distinct newest shard.
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.rotate_if_needed(t0 + Duration::from_secs(2));
        // "a" is in the (now older) front shard, not the back.
        let newest_before = s.shards.back().unwrap().count();
        assert!(s.contains_no_refresh(&v("a")));
        let newest_after = s.shards.back().unwrap().count();
        assert_eq!(
            newest_before, newest_after,
            "contains_no_refresh must not seed the newest shard"
        );
        // Sanity: the refreshing variant *does* seed it.
        assert!(s.contains(&v("a")));
        assert!(s.shards.back().unwrap().contains(&v("a")));
    }

    // -------------------- RollingBloomStorage --------------------

    #[test]
    fn rolling_bloom_drops_oldest_shard_on_rotate() {
        // ttl=4s, generations=4 → 1s per shard.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.shards.back_mut().unwrap().insert(&v("old"));
        s.rotate_if_needed(t0 + Duration::from_secs(5));
        assert_eq!(s.shards.len(), 4);
        assert!(
            !s.shards.iter().any(|sh| sh.contains(&v("old"))),
            "'old' should have rolled out of the window"
        );
    }

    #[test]
    fn rolling_bloom_refresh_on_contains_seeds_newest_shard() {
        // Force one rotation so `hot` lives in the front shard and the back
        // is fresh-empty. A real `contains` call must re-seed it into the
        // newest shard — this is what gives hot values survival across
        // future rotations.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        s.shards.back_mut().unwrap().insert(&v("hot"));
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.rotate_if_needed(t0 + Duration::from_secs(2));

        assert_eq!(
            s.shards.back().unwrap().count(),
            0,
            "back shard should be fresh-empty after rotation"
        );

        assert!(s.contains(&v("hot")));
        assert!(
            s.shards.back().unwrap().contains(&v("hot")),
            "contains() must re-seed found values into the newest shard"
        );
    }

    #[test]
    fn rolling_bloom_catch_up_capped_to_generations() {
        // Long idle period: ensure rotate_if_needed doesn't spin past
        // `generations` even if the elapsed time covers many windows.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.shards.back_mut().unwrap().insert(&v("stale"));
        // 1 hour gap: should rotate exactly `generations` times.
        s.rotate_if_needed(t0 + Duration::from_secs(3600));
        assert_eq!(s.shards.len(), 4, "deque size capped at `generations`");
        assert!(
            !s.shards.iter().any(|sh| sh.contains(&v("stale"))),
            "stale value flushed after long idle"
        );
    }

    #[test]
    fn rolling_bloom_slice_floors_to_one_second() {
        // ttl=2s, generations=8 → naive slice = 250ms; floor to 1s.
        let s = RollingBloomStorage::new(default_cache_size(), 8, Duration::from_secs(2));
        assert!(s.slice >= Duration::from_secs(1));
    }

    #[test]
    fn rolling_bloom_generations_clamped_to_at_least_one() {
        // generations=0 would imply div-by-zero or an empty deque; ensure
        // the constructor clamps it so we always have at least one shard.
        let s = RollingBloomStorage::new(default_cache_size(), 0, Duration::from_secs(60));
        assert_eq!(s.generations, 1);
        assert_eq!(s.shards.len(), 1);
    }
}

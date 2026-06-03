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
    event::metric::TagValueSet, internal_events::TagCardinalityTtlExpired,
    transforms::tag_cardinality_limit::config::Mode,
};

/// `Instant + Duration` panics outside the platform's representable range.
/// On overflow, push the deadline `~136 years` into the future so rotation
/// schedules degrade to a stable no-op rather than panicking or churning
/// (returning `instant` here would leave `next_rotate <= now` on every call
/// and force `generations` rotations per access).
fn saturating_add(instant: Instant, duration: Duration) -> Instant {
    if let Some(result) = instant.checked_add(duration) {
        return result;
    }
    let mut fallback = Duration::from_secs(u32::MAX as u64);
    while !fallback.is_zero() {
        if let Some(result) = instant.checked_add(fallback) {
            return result;
        }
        fallback = Duration::from_secs(fallback.as_secs() / 2);
    }
    instant
}

/// Compute time-slice count and duration so `slice * generations == ttl` exactly.
///
/// Caps `generations` to at most one slice per second of TTL (so each slice is
/// `>= 1s` when `ttl >= 1s`) and to the caller's `ttl_generations` request.
/// Uses `Duration` division for `slice` so non-divisible TTLs (e.g. 10s / 4)
/// still sum to the configured window. `ttl.as_secs()` is kept in `u64` when
/// deriving the per-second cap so values `>= 2^32` seconds are not truncated
/// to zero by a premature `u32` cast.
fn compute_ttl_slices(ttl: Duration, ttl_generations: u8) -> (u32, Duration) {
    let requested = u32::from(ttl_generations.max(1));
    let ttl_secs = ttl.as_secs().max(1);
    let max_for_ttl = ttl_secs.min(u32::MAX as u64) as u32;
    let generations = requested.min(max_for_ttl).max(1);
    let slice = ttl / generations;
    (generations, slice)
}

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
        // Write bits unconditionally so the rolling-bloom refresh path can
        // not leave a value riding on another value's false-positive bits.
        // Count tracks distinct first sightings only.
        let was_already_present = self.inner.contains(value);
        self.inner.insert(value);
        if !was_already_present {
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

/// `HashMap`-backed exact cache with per-value last-seen timestamps.
///
/// At most once per `sweep_interval` (= `ttl / generations`, after capping),
/// `retain` drops every entry whose `last_seen` is older than `ttl`. The
/// sweep runs lazily inside `insert`/`contains`/`len` — no background task.
struct TtlExactStorage {
    map: HashMap<TagValueSet, Instant>,
    ttl: Duration,
    sweep_interval: Duration,
    last_sweep: Instant,
}

impl TtlExactStorage {
    fn new(ttl: Duration, generations: u8) -> Self {
        // Cap generations so `sweep_interval >= 1s` and
        // `sweep_interval * generations == ttl`. Eviction precision is then
        // `[ttl, ttl + sweep_interval)`.
        let (_generations, sweep_interval) = compute_ttl_slices(ttl, generations);
        Self {
            map: HashMap::new(),
            ttl,
            sweep_interval,
            last_sweep: Instant::now(),
        }
    }

    fn maybe_sweep(&mut self, now: Instant) {
        if now.duration_since(self.last_sweep) < self.sweep_interval {
            return;
        }
        self.sweep(now);
    }

    /// Drop entries whose own `last_seen` is outside the TTL window. Runs even
    /// when `maybe_sweep` is not yet due so a lazy sweep interval cannot leave
    /// expired values visible to `contains` / `len`.
    fn purge_expired_entries(&mut self, now: Instant) {
        self.map
            .retain(|_, last_seen| now.duration_since(*last_seen) <= self.ttl);
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
        self.contains_with_now(value, Instant::now())
    }

    fn contains_with_now(&mut self, value: &TagValueSet, now: Instant) -> bool {
        self.maybe_sweep(now);
        self.purge_expired_entries(now);
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
        self.contains_no_refresh_with_now(value, Instant::now())
    }

    fn contains_no_refresh_with_now(&mut self, value: &TagValueSet, now: Instant) -> bool {
        self.maybe_sweep(now);
        self.purge_expired_entries(now);
        self.map.contains_key(value)
    }

    fn insert(&mut self, value: TagValueSet) {
        let now = Instant::now();
        self.maybe_sweep(now);
        self.purge_expired_entries(now);
        self.map.insert(value, now);
    }

    fn len(&mut self) -> usize {
        let now = Instant::now();
        self.maybe_sweep(now);
        self.purge_expired_entries(now);
        self.map.len()
    }
}

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
    /// Boundary at which the next rotation is due. Advances by `slice` on
    /// every rotation; storing the next tick (not the last) keeps the
    /// catch-up loop in `rotate_if_needed` trivial.
    next_rotate: Instant,
}

impl RollingBloomStorage {
    fn new(cache_size_per_key: usize, generations: u8, ttl: Duration) -> Self {
        // Cap generations so `slice >= 1s` and `slice * generations == ttl`.
        let (generations, slice) = compute_ttl_slices(ttl, generations);
        let mut shards = VecDeque::with_capacity(generations as usize);
        shards.push_back(BloomFilterStorage::new(cache_size_per_key));
        let now = Instant::now();
        Self {
            shards,
            generations: generations as u8,
            slice,
            cache_size_per_key,
            next_rotate: saturating_add(now, slice),
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
            self.next_rotate = saturating_add(self.next_rotate, self.slice);
            rotations += 1;
        }
        // If we needed more rotations than `generations`, the whole window is
        // stale — fast-forward `next_rotate` to avoid a tight catch-up the next
        // call after a long idle period.
        if now >= self.next_rotate {
            self.next_rotate = saturating_add(now, self.slice);
        }
    }

    fn contains(&mut self, value: &TagValueSet) -> bool {
        let now = Instant::now();
        self.rotate_if_needed(now);
        // Newest -> oldest short-circuits hot values; re-seed hits into the
        // newest shard so they survive the next rotation.
        let found = self.shards.iter().rev().any(|s| s.contains(value));
        if found && let Some(newest) = self.shards.back_mut() {
            newest.insert(value);
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

    /// Strict upper bound on the number of distinct values currently retained.
    ///
    /// Bloom shards are not enumerable, so the true union cardinality is not
    /// directly computable. Summing per-shard counts never under-counts; the
    /// alternative — `max` — could let distinct values spread across shards
    /// silently exceed `value_limit`. See the `ttl` section of the transform
    /// documentation for the over-rejection trade-off.
    fn len(&mut self) -> usize {
        let now = Instant::now();
        self.rotate_if_needed(now);
        self.shards.iter().map(|s| s.count()).sum()
    }
}

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
    /// Construct the appropriate backend from `(mode, ttl_secs, ttl_generations)`.
    ///
    /// When `ttl_secs` is `None` or `0`, this is identical to the pre-TTL
    /// behavior — `HashSet` for exact, single `BloomFilter` for probabilistic —
    /// so existing configs see zero behavioral change.
    pub fn new(mode: &Mode, ttl_secs: Option<u64>, ttl_generations: u8) -> Self {
        let ttl = ttl_secs.and_then(|s| (s > 0).then(|| Duration::from_secs(s)));

        let storage = match (mode, ttl) {
            (Mode::Exact, None) => TagValueSetStorage::Set(HashSet::new()),
            (Mode::Exact, Some(ttl)) => {
                TagValueSetStorage::TtlSet(TtlExactStorage::new(ttl, ttl_generations))
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
    pub(crate) const fn ttl_enabled(&self) -> bool {
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
    fn bloom_filter_storage_count_is_idempotent_per_value() {
        let mut b = BloomFilterStorage::new(default_cache_size());
        b.insert(&v("a"));
        b.insert(&v("a"));
        assert_eq!(b.count(), 1, "duplicate insert must not bump count");
        b.insert(&v("b"));
        assert_eq!(b.count(), 2);
    }

    #[test]
    fn exact_no_ttl_preserves_today_behavior() {
        let mut set = AcceptedTagValueSet::new(&Mode::Exact, None, 4);
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
        let mut set = AcceptedTagValueSet::new(&mode, None, 4);
        set.insert(v("a"));
        set.insert(v("a"));
        assert_eq!(set.len(), 1, "duplicate insert must not bump count");
        set.insert(v("b"));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&v("a")));
        assert!(set.contains(&v("b")));
    }

    // The storage types are exercised directly so we can drive
    // `Instant`-typed parameters on private helpers; `AcceptedTagValueSet`
    // itself calls `Instant::now()`, which can't be mocked cheaply.

    #[test]
    fn ttl_exact_expires_values_past_ttl() {
        let ttl = Duration::from_secs(60);
        let mut s = TtlExactStorage::new(ttl, 4);
        let t0 = Instant::now();
        s.map.insert(v("a"), t0);
        s.last_sweep = t0;
        s.sweep(t0 + Duration::from_secs(30));
        assert!(s.map.contains_key(&v("a")), "still alive within ttl");
        s.sweep(t0 + Duration::from_secs(90));
        assert!(!s.map.contains_key(&v("a")), "evicted past ttl");
    }

    #[test]
    fn ttl_exact_contains_does_not_refresh_expired_entry() {
        // Between sweeps, `contains` must not extend a value whose own
        // `last_seen + ttl` has passed, even if `maybe_sweep` has not run yet.
        let ttl = Duration::from_secs(60);
        let mut s = TtlExactStorage::new(ttl, 4);
        let t0 = Instant::now();
        s.map.insert(v("a"), t0);
        s.last_sweep = t0;
        let t50 = t0 + Duration::from_secs(50);
        s.sweep(t50);
        assert!(s.map.contains_key(&v("a")), "still within ttl at t=50");
        let t61 = t0 + Duration::from_secs(61);
        assert!(
            !s.contains_with_now(&v("a"), t61),
            "expired value must not be refreshed into the window"
        );
        assert!(
            !s.map.contains_key(&v("a")),
            "expired entry must be purged on access"
        );
    }

    #[test]
    fn ttl_exact_refresh_on_contains_extends_lease() {
        // Short sleep guarantees `Instant::now()` is strictly after `t_insert`
        // on every platform.
        let mut s = TtlExactStorage::new(Duration::from_secs(60), 4);
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
    fn ttl_exact_caps_generations_when_ttl_lt_generations() {
        // ttl=1s, generations=4 → effective=1, sweep_interval=1s.
        let s = TtlExactStorage::new(Duration::from_secs(1), 4);
        assert_eq!(s.sweep_interval, Duration::from_secs(1));
        // ttl=2s, generations=8 → effective=2, sweep_interval=1s.
        let s = TtlExactStorage::new(Duration::from_secs(2), 8);
        assert_eq!(s.sweep_interval, Duration::from_secs(1));
        // ttl >> generations is unaffected by the cap.
        let s = TtlExactStorage::new(Duration::from_secs(3600), 4);
        assert_eq!(s.sweep_interval, Duration::from_secs(900));
    }

    #[test]
    fn ttl_exact_contains_no_refresh_does_not_extend_lease() {
        let ttl = Duration::from_secs(60);
        let mut s = TtlExactStorage::new(ttl, 4);
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
        // Sanity: the refreshing variant must move the timestamp forward.
        s.contains(&v("a"));
        assert!(s.map.get(&v("a")).copied().unwrap() >= t0);
    }

    #[test]
    fn rolling_bloom_contains_no_refresh_does_not_seed_newest_shard() {
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        s.shards.back_mut().unwrap().insert(&v("a"));
        // Drive a rotation so "a" sits in the (now older) front shard.
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.rotate_if_needed(t0 + Duration::from_secs(2));
        let newest_before = s.shards.back().unwrap().count();
        assert!(s.contains_no_refresh(&v("a")));
        let newest_after = s.shards.back().unwrap().count();
        assert_eq!(
            newest_before, newest_after,
            "contains_no_refresh must not seed the newest shard"
        );
        // Sanity: the refreshing variant must seed it.
        assert!(s.contains(&v("a")));
        assert!(s.shards.back().unwrap().contains(&v("a")));
    }

    #[test]
    fn rolling_bloom_drops_oldest_shard_on_rotate() {
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
        // After one rotation, `hot` lives in the front shard and the back is
        // fresh-empty; `contains` must re-seed it into the newest shard.
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
        // After a long idle gap, `rotate_if_needed` must rotate at most
        // `generations` times even if elapsed covers many windows.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.shards.back_mut().unwrap().insert(&v("stale"));
        s.rotate_if_needed(t0 + Duration::from_secs(3600));
        assert_eq!(s.shards.len(), 4, "deque size capped at `generations`");
        assert!(
            !s.shards.iter().any(|sh| sh.contains(&v("stale"))),
            "stale value flushed after long idle"
        );
    }

    #[test]
    fn rolling_bloom_generations_clamped_to_at_least_one() {
        // `generations: 0` would divide by zero / leave an empty deque.
        let s = RollingBloomStorage::new(default_cache_size(), 0, Duration::from_secs(60));
        assert_eq!(s.generations, 1);
        assert_eq!(s.shards.len(), 1);
    }

    #[test]
    fn rolling_bloom_caps_generations_when_ttl_lt_generations() {
        // ttl=1s, generations=4 → effective=1, slice=1s.
        let s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(1));
        assert_eq!(s.generations, 1, "effective generations capped to ttl");
        assert_eq!(s.slice, Duration::from_secs(1));
        // ttl=2s, generations=8 → effective=2, slice=1s.
        let s = RollingBloomStorage::new(default_cache_size(), 8, Duration::from_secs(2));
        assert_eq!(s.generations, 2);
        assert_eq!(s.slice, Duration::from_secs(1));
    }

    #[test]
    fn rolling_bloom_window_matches_ttl_exactly() {
        // `slice * generations == ttl` must hold for every valid
        // (ttl_secs, ttl_generations), including non-divisible TTLs (e.g. 10 / 4).
        for (ttl_secs, generations) in [
            (1u64, 4u8),
            (2, 8),
            (3, 4),
            (10, 4),
            (60, 4),
            (3600, 4),
            (86400, 6),
            (4294967296, 4),
        ] {
            let s = RollingBloomStorage::new(
                default_cache_size(),
                generations,
                Duration::from_secs(ttl_secs),
            );
            assert_eq!(
                s.slice * u32::from(s.generations),
                Duration::from_secs(ttl_secs),
                "ttl_secs={ttl_secs}, generations={generations}: window must equal ttl",
            );
            assert!(
                s.slice >= Duration::from_secs(1),
                "ttl_secs={ttl_secs}, generations={generations}: slice must be >= 1s",
            );
        }
    }

    #[test]
    fn compute_ttl_slices_non_divisible_ttl_uses_fractional_slices() {
        // Integer seconds division would yield 10/4=2s slices and an ~8s window;
        // Duration division must preserve the full 10s contract.
        let (_, slice) = compute_ttl_slices(Duration::from_secs(10), 4);
        assert_eq!(slice, Duration::from_millis(2500));
        assert_eq!(slice * 4, Duration::from_secs(10));
    }

    #[test]
    fn compute_ttl_slices_large_ttl_secs_avoids_u32_truncation() {
        // `ttl.as_secs() as u32` wraps at 2^32, collapsing the generation cap
        // to 1 and changing rotation cadence.
        let ttl = Duration::from_secs(4294967296);
        let (generations, slice) = compute_ttl_slices(ttl, 4);
        assert_eq!(generations, 4, "requested generations must not collapse to 1");
        assert_eq!(slice * generations, ttl);

        let rolling = RollingBloomStorage::new(default_cache_size(), 4, ttl);
        assert_eq!(rolling.generations, 4);
        assert_eq!(rolling.slice * u32::from(rolling.generations), ttl);

        let exact = TtlExactStorage::new(ttl, 4);
        assert_eq!(exact.sweep_interval * 4, ttl);
    }

    #[test]
    fn rolling_bloom_hot_value_survives_rotation_after_contains_refresh() {
        // If refresh skipped insert when the newest shard already matched via
        // false positive, the value could vanish once the older shard rotated
        // out. Pollute the newest shard heavily so FP hits are likely, then
        // verify a front-shard-only value survives rotation after `contains()`
        // refresh.
        const TINY: usize = 128;
        let mut s = RollingBloomStorage::new(TINY, 2, Duration::from_secs(120));
        let hot = v("hot-value-survivor");
        s.shards.back_mut().unwrap().insert(&hot);
        let t0 = Instant::now();
        s.next_rotate = t0 + Duration::from_secs(1);
        s.rotate_if_needed(t0 + Duration::from_secs(2));
        assert!(
            s.shards.front().unwrap().contains(&hot),
            "after one rotation hot must live in the front shard"
        );
        for i in 0..512 {
            s.shards.back_mut().unwrap().insert(&v(&format!("pollute-{i}")));
        }
        assert!(s.contains(&hot), "must hit the front shard and refresh");
        assert!(
            s.shards.back().unwrap().contains(&hot),
            "refresh must unconditionally seed the newest shard"
        );
        s.rotate_if_needed(t0 + Duration::from_secs(62));
        assert!(
            s.contains_no_refresh(&hot),
            "hot must survive after the front shard carrying the original copy rotates out"
        );
    }

    #[test]
    fn rolling_bloom_len_sums_across_shards() {
        // Distinct values spread across shards must contribute to `len()`;
        // otherwise the union could silently exceed `value_limit`.
        let mut s = RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(4));
        s.shards.clear();
        for name in ["a", "b", "c", "d"] {
            let mut shard = BloomFilterStorage::new(default_cache_size());
            shard.insert(&v(name));
            s.shards.push_back(shard);
        }
        // Push the next rotation far out so `len()` doesn't lazily rotate.
        s.next_rotate = Instant::now() + Duration::from_secs(3600);
        assert_eq!(
            s.len(),
            4,
            "len() must sum per-shard counts to reflect the union upper bound"
        );
    }

    #[test]
    fn rolling_bloom_oversized_ttl_doesnt_panic() {
        let mut s =
            RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(u64::MAX));
        // Exercises both `saturating_add` call sites (constructor and
        // `rotate_if_needed`).
        s.insert(&v("a"));
        assert!(s.contains(&v("a")));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn saturating_add_overflow_pushes_deadline_far_into_future() {
        // The fallback must advance `instant` by a non-trivial amount —
        // returning `instant` itself would leave `next_rotate <= now` on
        // every access and force `generations` rotations per call.
        let now = Instant::now();
        let advanced = saturating_add(now, Duration::from_secs(u64::MAX));
        let gain = advanced.duration_since(now);
        assert!(
            gain >= Duration::from_secs(60 * 60 * 24 * 365),
            "saturating_add must push the deadline at least a year out on \
             overflow; got {gain:?}",
        );
    }

    #[test]
    fn rolling_bloom_overflow_does_not_churn_on_repeated_access() {
        // Repeated reads with an overflowing TTL must not silently rotate
        // out values inserted between them; the rotation deadline has to
        // sit far enough in the future that `rotate_if_needed` is a no-op.
        let mut s =
            RollingBloomStorage::new(default_cache_size(), 4, Duration::from_secs(u64::MAX));
        s.insert(&v("a"));
        for _ in 0..16 {
            assert!(s.contains(&v("a")));
        }
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn rolling_bloom_len_upper_bounds_value_limit() {
        // `len()` must reach `value_limit` once enough distinct values are
        // admitted across the full window so `try_accept_tag` stops admitting.
        let value_limit = 8usize;
        let generations = 4u8;
        let mut s =
            RollingBloomStorage::new(default_cache_size(), generations, Duration::from_secs(4));
        s.shards.clear();
        let per_shard = value_limit / generations as usize;
        let mut next = 0usize;
        for _ in 0..generations {
            let mut shard = BloomFilterStorage::new(default_cache_size());
            for _ in 0..per_shard {
                shard.insert(&v(&format!("v{next}")));
                next += 1;
            }
            s.shards.push_back(shard);
        }
        s.next_rotate = Instant::now() + Duration::from_secs(3600);
        assert!(
            s.len() >= value_limit,
            "len() must reach value_limit once enough distinct values are spread \
             across shards; got {} for value_limit={value_limit}",
            s.len(),
        );
    }
}

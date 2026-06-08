use std::{
    marker::PhantomData,
    time::{Duration, Instant},
};

use indexmap::IndexMap;
use lru::LruCache;
use serde_with::serde_as;
use snafu::Snafu;
use vector_config_macros::configurable_component;
use vector_lib::{
    ByteSizeOf,
    event::{
        EventMetadata, Metric, MetricKind,
        metric::{MetricData, MetricSeries},
    },
};

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum NormalizerError {
    #[snafu(display("`max_bytes` must be greater than zero"))]
    InvalidMaxBytes,
    #[snafu(display("`max_events` must be greater than zero"))]
    InvalidMaxEvents,
    #[snafu(display("`time_to_live` must be greater than zero"))]
    InvalidTimeToLive,
}

/// Defines behavior for creating the MetricNormalizer
#[serde_as]
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Default)]
pub struct NormalizerConfig<D: NormalizerSettings + Clone> {
    /// The maximum size in bytes of the events in the metrics normalizer cache, excluding cache overhead.
    #[serde(default = "default_max_bytes::<D>")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_bytes: Option<usize>,

    /// The maximum number of events of the metrics normalizer cache
    #[serde(default = "default_max_events::<D>")]
    #[configurable(metadata(docs::type_unit = "events"))]
    pub max_events: Option<usize>,

    /// The maximum age of a metric not being updated before it is evicted from the metrics normalizer cache.
    #[serde(default = "default_time_to_live::<D>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Time To Live"))]
    pub time_to_live: Option<u64>,

    #[serde(skip)]
    pub _d: PhantomData<D>,
}

const fn default_max_bytes<D: NormalizerSettings>() -> Option<usize> {
    D::MAX_BYTES
}

const fn default_max_events<D: NormalizerSettings>() -> Option<usize> {
    D::MAX_EVENTS
}

const fn default_time_to_live<D: NormalizerSettings>() -> Option<u64> {
    D::TIME_TO_LIVE
}

impl<D: NormalizerSettings + Clone> NormalizerConfig<D> {
    pub fn validate(&self) -> Result<NormalizerConfig<D>, NormalizerError> {
        let config = NormalizerConfig::<D> {
            max_bytes: self.max_bytes.or(D::MAX_BYTES),
            max_events: self.max_events.or(D::MAX_EVENTS),
            time_to_live: self.time_to_live.or(D::TIME_TO_LIVE),
            _d: Default::default(),
        };
        match (config.max_bytes, config.max_events, config.time_to_live) {
            (Some(0), _, _) => Err(NormalizerError::InvalidMaxBytes),
            (_, Some(0), _) => Err(NormalizerError::InvalidMaxEvents),
            (_, _, Some(0)) => Err(NormalizerError::InvalidTimeToLive),
            _ => Ok(config),
        }
    }

    pub const fn into_settings(self) -> MetricSetSettings {
        MetricSetSettings {
            max_bytes: self.max_bytes,
            max_events: self.max_events,
            time_to_live: self.time_to_live,
        }
    }
}

pub trait NormalizerSettings {
    const MAX_EVENTS: Option<usize>;
    const MAX_BYTES: Option<usize>;
    const TIME_TO_LIVE: Option<u64>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultNormalizerSettings;

impl NormalizerSettings for DefaultNormalizerSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = None;
    const TIME_TO_LIVE: Option<u64> = None;
}

/// Normalizes metrics according to a set of rules.
///
/// Depending on the system in which they are being sent to, metrics may have to be modified in order to fit the data
/// model or constraints placed on that system.  Typically, this boils down to whether or not the system can accept
/// absolute metrics or incremental metrics: the latest value of a metric, or the delta between the last time the
/// metric was observed and now, respective. Other rules may need to be applied, such as dropping metrics of a specific
/// type that the system does not support.
///
/// The trait provides a simple interface to apply this logic uniformly, given a reference to a simple state container
/// that allows tracking the necessary information of a given metric over time. As well, given the optional return, it
/// composes nicely with iterators (i.e. using `filter_map`) in order to filter metrics within existing
/// iterator/stream-based approaches.
pub trait MetricNormalize {
    /// Normalizes the metric against the given state.
    ///
    /// If the metric was normalized successfully, `Some(metric)` will be returned. Otherwise, `None` is returned.
    ///
    /// In some cases, a metric may be successfully added/tracked within the given state, but due to the normalization
    /// logic, it cannot yet be emitted. An example of this is normalizing all metrics to be incremental.
    ///
    /// In this example, if an incoming metric is already incremental, it can be passed through unchanged.  If the
    /// incoming metric is absolute, however, we need to see it at least twice in order to calculate the incremental
    /// delta necessary to emit an incremental version. This means that the first time an absolute metric is seen,
    /// `normalize` would return `None`, and the subsequent calls would return `Some(metric)`.
    ///
    /// However, a metric may simply not be supported by a normalization implementation, and so `None` may or may not be
    /// a common return value. This behavior is, thus, implementation defined.
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric>;
}

/// A self-contained metric normalizer.
///
/// The normalization state is stored internally, and it can only be created from a normalizer implementation that is
/// either `Default` or is constructed ahead of time, so it is primarily useful for constructing a usable normalizer
/// via implicit conversion methods or when no special parameters are required for configuring the underlying normalizer.
pub struct MetricNormalizer<N> {
    state: MetricSet,
    normalizer: N,
}

impl<N> MetricNormalizer<N> {
    /// Creates a new normalizer with the given configuration.
    pub fn with_config<D: NormalizerSettings + Clone>(
        normalizer: N,
        config: NormalizerConfig<D>,
    ) -> Self {
        let settings = config
            .validate()
            .unwrap_or_else(|e| panic!("Invalid cache settings: {e:?}"))
            .into_settings();
        Self {
            state: MetricSet::new(settings),
            normalizer,
        }
    }

    /// Gets a mutable reference to the current metric state for this normalizer.
    pub const fn get_state_mut(&mut self) -> &mut MetricSet {
        &mut self.state
    }
}

impl<N: MetricNormalize> MetricNormalizer<N> {
    /// Normalizes the metric against the internal normalization state.
    ///
    /// For more information about normalization, see the documentation for [`MetricNormalize::normalize`].
    pub fn normalize(&mut self, metric: Metric) -> Option<Metric> {
        self.normalizer.normalize(&mut self.state, metric)
    }
}

impl<N: Default> Default for MetricNormalizer<N> {
    fn default() -> Self {
        Self {
            state: MetricSet::default(),
            normalizer: N::default(),
        }
    }
}

impl<N> From<N> for MetricNormalizer<N> {
    fn from(normalizer: N) -> Self {
        Self {
            state: MetricSet::default(),
            normalizer,
        }
    }
}

/// Represents a stored metric entry with its data, metadata, and timestamp.
#[derive(Clone, Debug)]
pub struct MetricEntry {
    /// The metric data containing the value and kind
    pub data: MetricData,
    /// Event metadata associated with this metric
    pub metadata: EventMetadata,
    /// Optional timestamp for TTL tracking
    pub timestamp: Option<Instant>,
}

impl ByteSizeOf for MetricEntry {
    fn allocated_bytes(&self) -> usize {
        self.data.allocated_bytes() + self.metadata.allocated_bytes()
    }
}

impl MetricEntry {
    /// Creates a new MetricEntry with the given data, metadata, and timestamp.
    pub const fn new(
        data: MetricData,
        metadata: EventMetadata,
        timestamp: Option<Instant>,
    ) -> Self {
        Self {
            data,
            metadata,
            timestamp,
        }
    }

    /// Creates a new MetricEntry from a Metric.
    pub fn from_metric(metric: Metric, timestamp: Option<Instant>) -> (MetricSeries, Self) {
        let (series, data, metadata) = metric.into_parts();
        let entry = Self::new(data, metadata, timestamp);
        (series, entry)
    }

    /// Converts this entry back to a Metric with the given series.
    pub fn into_metric(self, series: MetricSeries) -> Metric {
        Metric::from_parts(series, self.data, self.metadata)
    }

    /// Updates this entry's timestamp.
    pub const fn update_timestamp(&mut self, timestamp: Option<Instant>) {
        self.timestamp = timestamp;
    }

    /// Checks if this entry has expired based on the given TTL and reference time.
    ///
    /// Using a provided reference time ensures consistency across multiple expiration checks.
    pub fn is_expired(&self, ttl: Duration, reference_time: Instant) -> bool {
        match self.timestamp {
            Some(ts) => reference_time.duration_since(ts) >= ttl,
            None => false,
        }
    }
}

/// Configuration for capacity-based eviction (memory and/or entry count limits).
#[derive(Clone, Debug)]
pub struct CapacityPolicy {
    /// Maximum memory usage in bytes
    pub max_bytes: Option<usize>,
    /// Maximum number of entries
    pub max_events: Option<usize>,
    /// Current memory usage tracking
    current_memory: usize,
}

impl CapacityPolicy {
    /// Creates a new capacity policy with both memory and entry limits.
    pub const fn new(max_bytes: Option<usize>, max_events: Option<usize>) -> Self {
        Self {
            max_bytes,
            max_events,
            current_memory: 0,
        }
    }

    /// Gets the current memory usage.
    pub const fn current_memory(&self) -> usize {
        self.current_memory
    }

    /// Updates memory tracking when an entry is removed.
    const fn remove_memory(&mut self, bytes: usize) {
        self.current_memory = self.current_memory.saturating_sub(bytes);
    }

    /// Frees the memory for an item if max_bytes is set.
    /// Only calculates and tracks memory when max_bytes is specified.
    pub fn free_item(&mut self, series: &MetricSeries, entry: &MetricEntry) {
        if self.max_bytes.is_some() {
            let freed_memory = self.item_size(series, entry);
            self.remove_memory(freed_memory);
        }
    }

    /// Updates memory tracking.
    const fn replace_memory(&mut self, old_bytes: usize, new_bytes: usize) {
        self.current_memory = self
            .current_memory
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
    }

    /// Checks if the current state exceeds memory limits.
    const fn exceeds_memory_limit(&self) -> bool {
        if let Some(max_bytes) = self.max_bytes {
            self.current_memory > max_bytes
        } else {
            false
        }
    }

    /// Checks if the given entry count exceeds entry limits.
    const fn exceeds_entry_limit(&self, entry_count: usize) -> bool {
        if let Some(max_events) = self.max_events {
            entry_count > max_events
        } else {
            false
        }
    }

    /// Returns true if any limits are currently exceeded.
    const fn needs_eviction(&self, entry_count: usize) -> bool {
        self.exceeds_memory_limit() || self.exceeds_entry_limit(entry_count)
    }

    /// Gets the total memory size of entry/series, excluding LRU cache overhead.
    pub fn item_size(&self, series: &MetricSeries, entry: &MetricEntry) -> usize {
        entry.allocated_bytes() + series.allocated_bytes()
    }
}

#[derive(Clone, Debug)]
pub struct TtlPolicy {
    /// Time-to-live for entries
    pub ttl: Duration,
    /// How often to run cleanup
    pub cleanup_interval: Duration,
    /// Last time cleanup was performed
    pub(crate) last_cleanup: Instant,
}

/// Configuration for automatic cleanup of expired entries.
impl TtlPolicy {
    /// Creates a new TTL policy with the given duration.
    /// Cleanup interval defaults to TTL/10 with a 10-second minimum.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            cleanup_interval: ttl.div_f32(10.0).max(Duration::from_secs(10)),
            last_cleanup: Instant::now(),
        }
    }

    /// Checks if it's time to run cleanup.
    ///
    /// Returns Some(current_time) if cleanup should be performed, None otherwise.
    pub fn should_cleanup(&self) -> Option<Instant> {
        let now = Instant::now();
        if now.duration_since(self.last_cleanup) >= self.cleanup_interval {
            Some(now)
        } else {
            None
        }
    }

    /// Marks cleanup as having been performed with the provided timestamp.
    pub const fn mark_cleanup_done(&mut self, now: Instant) {
        self.last_cleanup = now;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MetricSetSettings {
    pub max_bytes: Option<usize>,
    pub max_events: Option<usize>,
    pub time_to_live: Option<u64>,
}

/// Inner storage for `MetricSet`.
///
/// Uses `IndexMap` when no capacity eviction policy is configured — avoiding the
/// per-access LRU bookkeeping (pointer chasing in a doubly-linked list) that
/// `LruCache::get_mut` performs unconditionally.  `LruCache` is used only when a
/// capacity policy is set, so that LRU eviction order is maintained correctly.
#[derive(Clone, Debug)]
enum MetricSetInner {
    /// Unbounded storage with no eviction.  Hash-map lookup only, no LRU overhead.
    Unbounded(IndexMap<MetricSeries, MetricEntry>),
    /// Bounded storage with LRU eviction semantics.
    Bounded(LruCache<MetricSeries, MetricEntry>),
}

impl MetricSetInner {
    fn len(&self) -> usize {
        match self {
            Self::Unbounded(m) => m.len(),
            Self::Bounded(m) => m.len(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Unbounded(m) => m.is_empty(),
            Self::Bounded(m) => m.is_empty(),
        }
    }

    /// Returns a mutable reference to the entry.
    ///
    /// For `Unbounded` this is a plain hash-map lookup.
    /// For `Bounded` this also promotes the entry to the MRU end of the LRU list.
    fn get_mut(&mut self, key: &MetricSeries) -> Option<&mut MetricEntry> {
        match self {
            Self::Unbounded(m) => m.get_mut(key),
            Self::Bounded(m) => m.get_mut(key),
        }
    }

    /// Inserts or replaces an entry, returning the previous value if any.
    fn put(&mut self, key: MetricSeries, value: MetricEntry) -> Option<MetricEntry> {
        match self {
            Self::Unbounded(m) => m.insert(key, value),
            Self::Bounded(m) => m.put(key, value),
        }
    }

    /// Removes an entry by key, returning it if present.
    fn pop(&mut self, key: &MetricSeries) -> Option<MetricEntry> {
        match self {
            // swap_remove is O(1) vs shift_remove's O(n); insertion order is not required here.
            Self::Unbounded(m) => m.swap_remove(key),
            Self::Bounded(m) => m.pop(key),
        }
    }

    fn iter(&self) -> MetricSetIter<'_> {
        match self {
            Self::Unbounded(m) => MetricSetIter::Unbounded(m.iter()),
            Self::Bounded(m) => MetricSetIter::Bounded(m.iter()),
        }
    }
}

enum MetricSetIter<'a> {
    Unbounded(indexmap::map::Iter<'a, MetricSeries, MetricEntry>),
    Bounded(lru::Iter<'a, MetricSeries, MetricEntry>),
}

impl<'a> Iterator for MetricSetIter<'a> {
    type Item = (&'a MetricSeries, &'a MetricEntry);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Unbounded(it) => it.next(),
            Self::Bounded(it) => it.next(),
        }
    }
}

/// Dual-limit cache for metric normalization with optional capacity and TTL policies.
///
/// Uses `IndexMap` internally when no capacity eviction policy is configured, avoiding
/// the per-access LRU pointer-manipulation overhead of `LruCache`. Switches to
/// `LruCache` only when a `max_bytes` or `max_events` capacity policy is set, so that
/// LRU eviction ordering is preserved for those cases.
#[derive(Clone, Debug)]
pub struct MetricSet {
    inner: MetricSetInner,
    /// Optional capacity policy for memory and/or entry count limits
    capacity_policy: Option<CapacityPolicy>,
    /// Optional TTL policy for time-based expiration
    ttl_policy: Option<TtlPolicy>,
}

impl MetricSet {
    /// Creates a new MetricSet with the given settings.
    pub fn new(settings: MetricSetSettings) -> Self {
        // Create capacity policy if any capacity limit is set
        let capacity_policy = match (settings.max_bytes, settings.max_events) {
            (None, None) => None,
            (max_bytes, max_events) => Some(CapacityPolicy::new(max_bytes, max_events)),
        };

        // Create TTL policy if time-to-live is set
        let ttl_policy = settings
            .time_to_live
            .map(|ttl| TtlPolicy::new(Duration::from_secs(ttl)));

        Self::with_policies(capacity_policy, ttl_policy)
    }

    /// Creates a new MetricSet with the given policies.
    pub fn with_policies(
        capacity_policy: Option<CapacityPolicy>,
        ttl_policy: Option<TtlPolicy>,
    ) -> Self {
        // Use LruCache only when a capacity policy requires LRU eviction ordering.
        // Without a capacity policy, IndexMap avoids the per-access LRU overhead.
        let inner = if capacity_policy.is_some() {
            MetricSetInner::Bounded(LruCache::unbounded())
        } else {
            MetricSetInner::Unbounded(IndexMap::default())
        };
        Self {
            inner,
            capacity_policy,
            ttl_policy,
        }
    }

    /// Gets the current capacity policy.
    pub const fn capacity_policy(&self) -> Option<&CapacityPolicy> {
        self.capacity_policy.as_ref()
    }

    /// Gets the current TTL policy.
    pub const fn ttl_policy(&self) -> Option<&TtlPolicy> {
        self.ttl_policy.as_ref()
    }

    /// Gets a mutable reference to the TTL policy configuration.
    pub const fn ttl_policy_mut(&mut self) -> Option<&mut TtlPolicy> {
        self.ttl_policy.as_mut()
    }

    /// Gets the current number of entries in the cache.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Gets the current memory usage in bytes.
    pub fn weighted_size(&self) -> u64 {
        self.capacity_policy
            .as_ref()
            .map_or(0, |cp| cp.current_memory() as u64)
    }

    /// Creates a timestamp if TTL is enabled.
    fn create_timestamp(&self) -> Option<Instant> {
        self.ttl_policy.as_ref().map(|_| Instant::now())
    }

    /// Enforce memory and entry limits by evicting LRU entries.
    fn enforce_capacity_policy(&mut self) {
        let Some(ref mut capacity_policy) = self.capacity_policy else {
            return; // No capacity limits configured
        };

        // A capacity policy is only set when inner is Bounded; this should always be true.
        let MetricSetInner::Bounded(ref mut lru) = self.inner else {
            debug_assert!(false, "capacity policy set but inner is not Bounded");
            return;
        };

        // Keep evicting until we're within limits
        while capacity_policy.needs_eviction(lru.len()) {
            if let Some((series, entry)) = lru.pop_lru() {
                capacity_policy.free_item(&series, &entry);
            } else {
                break; // No more entries to evict
            }
        }
    }

    /// Perform TTL cleanup if configured and needed.
    fn maybe_cleanup(&mut self) {
        // Check if cleanup is needed and get the current timestamp in one operation
        let now = match self.ttl_policy().and_then(|config| config.should_cleanup()) {
            Some(timestamp) => timestamp,
            None => return, // No cleanup needed
        };

        // Perform the cleanup using the same timestamp
        self.cleanup_expired(now);

        // Mark cleanup as done with the same timestamp
        if let Some(config) = self.ttl_policy_mut() {
            config.mark_cleanup_done(now);
        }
    }

    /// Remove expired entries based on TTL using the provided timestamp.
    fn cleanup_expired(&mut self, now: Instant) {
        // Get the TTL from the policy
        let Some(ttl) = self.ttl_policy().map(|policy| policy.ttl) else {
            return; // No TTL policy, nothing to do
        };

        // Collect expired keys using the provided timestamp
        let expired_keys: Vec<MetricSeries> = self
            .inner
            .iter()
            .filter(|(_, e)| e.is_expired(ttl, now))
            .map(|(s, _)| s.clone())
            .collect();

        // Remove expired entries and update memory tracking (if max_bytes is set)
        for series in expired_keys {
            if let Some(entry) = self.inner.pop(&series)
                && let Some(ref mut capacity_policy) = self.capacity_policy
            {
                capacity_policy.free_item(&series, &entry);
            }
        }
    }

    /// Internal insert that updates memory tracking and enforces limits.
    fn insert_with_tracking(&mut self, series: MetricSeries, entry: MetricEntry) {
        let Some(ref mut capacity_policy) = self.capacity_policy else {
            self.inner.put(series, entry);
            return; // No capacity limits configured, return immediately
        };

        // Handle differently based on whether we need to track memory
        if capacity_policy.max_bytes.is_some() {
            // When tracking memory, we need to calculate sizes before and after
            let entry_size = capacity_policy.item_size(&series, &entry);

            if let Some(existing_entry) = self.inner.put(series.clone(), entry) {
                // If we had an existing entry, calculate its size and adjust memory tracking
                let existing_size = capacity_policy.item_size(&series, &existing_entry);
                capacity_policy.replace_memory(existing_size, entry_size);
            } else {
                // No existing entry, just add the new entry's size
                capacity_policy.replace_memory(0, entry_size);
            }
        } else {
            // When not tracking memory (only entry count limits), just put directly
            self.inner.put(series, entry);
        }

        // Enforce limits after insertion
        self.enforce_capacity_policy();
    }

    /// Consumes this MetricSet and returns a vector of Metric.
    pub fn into_metrics(mut self) -> Vec<Metric> {
        // Clean up expired entries first (using current time)
        self.cleanup_expired(Instant::now());
        match self.inner {
            MetricSetInner::Unbounded(m) => m
                .into_iter()
                .map(|(series, entry)| entry.into_metric(series))
                .collect(),
            MetricSetInner::Bounded(mut m) => {
                let mut metrics = Vec::with_capacity(m.len());
                while let Some((series, entry)) = m.pop_lru() {
                    metrics.push(entry.into_metric(series));
                }
                metrics
            }
        }
    }

    /// Either pass the metric through as-is if absolute, or convert it
    /// to absolute if incremental.
    pub fn make_absolute(&mut self, metric: Metric) -> Option<Metric> {
        self.maybe_cleanup();
        match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => Some(self.incremental_to_absolute(metric)),
        }
    }

    /// Either convert the metric to incremental if absolute, or
    /// aggregate it with any previous value if already incremental.
    pub fn make_incremental(&mut self, metric: Metric) -> Option<Metric> {
        self.maybe_cleanup();
        match metric.kind() {
            MetricKind::Absolute => self.absolute_to_incremental(metric),
            MetricKind::Incremental => Some(metric),
        }
    }

    /// Convert the incremental metric into an absolute one, using the
    /// state buffer to keep track of the value throughout the entire
    /// application uptime.
    fn incremental_to_absolute(&mut self, mut metric: Metric) -> Metric {
        let timestamp = self.create_timestamp();
        // We always call insert() to track memory usage
        match self.inner.get_mut(metric.series()) {
            Some(existing) => {
                let mut new_value = existing.data.value().clone();
                if new_value.add(metric.value()) {
                    // Update the stored value
                    metric = metric.with_value(new_value);
                }
                // Insert the updated stored value, or as store a new reference value (if the Metric changed type)
                let mut cache_metric = metric.clone();
                // Strip finalizers from the clone before caching. The normalization
                // cache must not hold Arc<EventFinalizer> references, as that
                // prevents the disk buffer from acknowledging events — leading to
                // a deadlock once the buffer fills and no new events can replace
                // cache entries.
                cache_metric.metadata_mut().take_finalizers();
                self.insert(cache_metric, timestamp);
            }
            None => {
                let mut cache_metric = metric.clone();
                cache_metric.metadata_mut().take_finalizers();
                self.insert(cache_metric, timestamp);
            }
        }
        metric.into_absolute()
    }

    /// Convert the absolute metric into an incremental by calculating
    /// the increment from the last saved absolute state.
    fn absolute_to_incremental(&mut self, mut metric: Metric) -> Option<Metric> {
        // NOTE: Crucially, like I did, you may wonder: why do we not always return a metric? Could
        // this lead to issues where a metric isn't seen again and we, in effect, never emit it?
        //
        // You're not wrong, and that does happen based on the logic below.  However, the main
        // problem this logic solves is avoiding massive counter updates when Vector restarts.
        //
        // If we emitted a metric for a newly-seen absolute metric in this method, we would
        // naturally have to emit an incremental version where the value was the absolute value,
        // with subsequent updates being only delta updates.  If we restarted Vector, however, we
        // would be back to not having yet seen the metric before, so the first emission of the
        // metric after converting it here would be... its absolute value.  Even if the value only
        // changed by 1 between Vector stopping and restarting, we could be incrementing the counter
        // by some outrageous amount.
        //
        // Thus, we only emit a metric when we've calculated an actual delta for it, which means
        // that, yes, we're risking never seeing a metric if it's not re-emitted, and we're
        // introducing a small amount of lag before a metric is emitted by having to wait to see it
        // again, but this is a behavior we have to observe for sinks that can only handle
        // incremental updates.
        let timestamp = self.create_timestamp();
        // We always call insert() to track memory usage
        match self.inner.get_mut(metric.series()) {
            Some(reference) => {
                let new_value = metric.value().clone();
                // Create a copy of the reference so we can insert and
                // replace the existing entry, tracking memory usage
                let mut new_reference = reference.clone();
                // From the stored reference value, emit an increment
                if metric.subtract(&reference.data) {
                    new_reference.data.value = new_value;
                    new_reference.timestamp = timestamp;
                    self.insert_with_tracking(metric.series().clone(), new_reference);
                    Some(metric.into_incremental())
                } else {
                    // Metric changed type, store this and emit nothing.
                    // Strip finalizers — the normalization cache must not hold
                    // Arc<EventFinalizer> references (see incremental_to_absolute).
                    metric.metadata_mut().take_finalizers();
                    self.insert(metric, timestamp);
                    None
                }
            }
            None => {
                // No reference so store this and emit nothing.
                // Strip finalizers before caching (see incremental_to_absolute).
                metric.metadata_mut().take_finalizers();
                self.insert(metric, timestamp);
                None
            }
        }
    }

    fn insert(&mut self, metric: Metric, timestamp: Option<Instant>) {
        let (series, entry) = MetricEntry::from_metric(metric, timestamp);
        self.insert_with_tracking(series, entry);
    }

    pub fn insert_update(&mut self, metric: Metric) {
        self.maybe_cleanup();
        let timestamp = self.create_timestamp();
        let update = match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => {
                // Incremental metrics update existing entries, if present
                match self.inner.get_mut(metric.series()) {
                    Some(existing) => {
                        // Create a copy of the reference so we can insert and
                        // replace the existing entry, tracking memory usage
                        let mut new_existing = existing.clone();
                        let (series, data, metadata) = metric.into_parts();
                        if new_existing.data.update(&data) {
                            new_existing.metadata.merge(metadata);
                            new_existing.update_timestamp(timestamp);
                            self.insert_with_tracking(series, new_existing);
                            None
                        } else {
                            warn!(message = "Metric changed type, dropping old value.", %series);
                            Some(Metric::from_parts(series, data, metadata))
                        }
                    }
                    None => Some(metric),
                }
            }
        };
        if let Some(metric) = update {
            self.insert(metric, timestamp);
        }
    }

    /// Removes a series from the cache.
    ///
    /// If the series existed and was removed, returns true.  Otherwise, false.
    pub fn remove(&mut self, series: &MetricSeries) -> bool {
        if let Some(entry) = self.inner.pop(series) {
            if let Some(ref mut capacity_policy) = self.capacity_policy {
                capacity_policy.free_item(series, &entry);
            }
            return true;
        }
        false
    }
}

impl Default for MetricSet {
    fn default() -> Self {
        Self::new(MetricSetSettings::default())
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::event::metric::{MetricKind, MetricValue};

    use super::*;

    fn counter(name: &str, value: f64, kind: MetricKind) -> Metric {
        Metric::new(name, kind, MetricValue::Counter { value })
    }

    // Verifies that the default (no capacity policy) path uses IndexMap and that
    // make_absolute / into_metrics behave correctly across multiple updates.
    #[test]
    fn unbounded_incremental_to_absolute_accumulates() {
        let mut set = MetricSet::default();
        assert!(matches!(set.inner, MetricSetInner::Unbounded(_)));

        // First incremental: stored as reference, emitted as absolute 1.0
        let out = set.make_absolute(counter("hits", 1.0, MetricKind::Incremental));
        assert_eq!(out.unwrap().value(), &MetricValue::Counter { value: 1.0 });

        // Second incremental: accumulated with previous, emitted as absolute 3.0
        let out = set.make_absolute(counter("hits", 2.0, MetricKind::Incremental));
        assert_eq!(out.unwrap().value(), &MetricValue::Counter { value: 3.0 });

        // into_metrics drains the set and returns all tracked series
        let metrics = set.into_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name(), "hits");
    }

    #[test]
    fn unbounded_absolute_passes_through() {
        let mut set = MetricSet::default();

        let out = set.make_absolute(counter("rps", 42.0, MetricKind::Absolute));
        assert_eq!(out.unwrap().value(), &MetricValue::Counter { value: 42.0 });

        // Absolute metrics are not stored in the set
        assert!(set.is_empty());
    }

    // Verifies that capacity policy switches to the LruCache (Bounded) path.
    #[test]
    fn bounded_path_selected_when_capacity_policy_set() {
        let set = MetricSet::new(MetricSetSettings {
            max_events: Some(10),
            ..Default::default()
        });
        assert!(matches!(set.inner, MetricSetInner::Bounded(_)));
    }
}

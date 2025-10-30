use std::{
    marker::PhantomData,
    time::{Duration, Instant},
};

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
/// model or constraints placed on that system. Typically, this boils down to whether or not the system can accept
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
        // Calculate the size of the data and metadata
        let data_size = self.data.allocated_bytes();
        let metadata_size = self.metadata.allocated_bytes();

        // Include struct overhead - size of self without double-counting fields
        // that we already accounted for in their respective allocated_bytes() calls
        let struct_size = size_of::<Self>();

        data_size + metadata_size + struct_size
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

    /// Frees the memory for an item, always tracking memory usage.
    /// Memory tracking now happens regardless of whether max_bytes is set.
    pub fn free_item(&mut self, series: &MetricSeries, entry: &MetricEntry) {
        let freed_memory = self.item_size(series, entry);
        self.remove_memory(freed_memory);
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

/// Dual-limit cache using standard LRU with optional capacity and TTL policies.
///
/// This implementation uses the standard LRU crate with optional enforcement of both
/// memory and entry count limits via CapacityPolicy, plus optional TTL via TtlPolicy.
#[derive(Clone, Debug)]
pub struct MetricSet {
    /// LRU cache for storing metric entries
    inner: LruCache<MetricSeries, MetricEntry>,
    /// Optional capacity policy for memory and/or entry count limits
    capacity_policy: Option<CapacityPolicy>,
    /// Optional TTL policy for time-based expiration
    ttl_policy: Option<TtlPolicy>,
    /// Counter for evictions. Used for metrics tracking
    eviction_count: usize,
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
        // Always use an unbounded cache since we manually track limits
        // This ensures our capacity policy can properly track memory for all evicted entries
        Self {
            inner: LruCache::unbounded(),
            capacity_policy,
            ttl_policy,
            eviction_count: 0,
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

        // Keep evicting until we're within limits
        while capacity_policy.needs_eviction(self.inner.len()) {
            if let Some((series, entry)) = self.inner.pop_lru() {
                capacity_policy.free_item(&series, &entry);
                self.eviction_count += 1;
            } else {
                break; // No more entries to evict
            }
        }
    }

    /// Reset the eviction count and return the previous value
    pub const fn get_and_reset_eviction_count(&mut self) -> usize {
        let count = self.eviction_count;
        self.eviction_count = 0;
        count
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

        let mut expired_keys = Vec::new();

        // Collect expired keys using the provided timestamp
        for (series, entry) in self.inner.iter() {
            if entry.is_expired(ttl, now) {
                expired_keys.push(series.clone());
            }
        }

        // Remove expired entries and update memory tracking (if max_bytes is set)
        for series in expired_keys {
            if let Some(entry) = self.inner.pop(&series) {
                self.eviction_count += 1;
                if let Some(ref mut capacity_policy) = self.capacity_policy {
                    capacity_policy.free_item(&series, &entry);
                }
            }
        }
    }

    /// Internal insert that updates memory tracking and enforces limits.
    fn insert_with_tracking(&mut self, series: MetricSeries, entry: MetricEntry) {
        let Some(ref mut capacity_policy) = self.capacity_policy else {
            self.inner.put(series, entry);
            return; // No capacity limits configured, return immediately
        };

        // Always track memory when capacity policy exists
        let entry_size = capacity_policy.item_size(&series, &entry);
        if let Some(existing_entry) = self.inner.put(series.clone(), entry) {
            // If we had an existing entry, calculate its size and adjust memory tracking
            let existing_size = capacity_policy.item_size(&series, &existing_entry);
            capacity_policy.replace_memory(existing_size, entry_size);
        } else {
            // No existing entry, just add the new entry's size
            capacity_policy.replace_memory(0, entry_size);
        }

        // Get item; move to back of LRU cache
        self.inner.get(&series);

        // Enforce limits after insertion
        self.enforce_capacity_policy();
    }

    /// Consumes this MetricSet and returns a vector of Metric.
    pub fn into_metrics(mut self) -> Vec<Metric> {
        // Clean up expired entries first (using current time)
        self.cleanup_expired(Instant::now());
        let mut metrics = Vec::new();
        while let Some((series, entry)) = self.inner.pop_lru() {
            metrics.push(entry.into_metric(series));
        }
        metrics
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
                self.insert(metric.clone(), timestamp);
            }
            None => {
                self.insert(metric.clone(), timestamp);
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
                    // Metric changed type, store this and emit nothing
                    self.insert(metric, timestamp);
                    None
                }
            }
            None => {
                // No reference so store this and emit nothing
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
    use vector_lib::{
        event::{
            Metric, MetricKind, MetricValue
        },
    };
    use super::*;
    use similar_asserts::assert_eq;

    // Helper function to create a metric with a unique name and value
    fn create_test_metric(name: &str, kind: MetricKind, value: MetricValue) -> Metric {
        Metric::new(
            name,
            kind,
            value,
        )
    }

    #[test]
    fn test_metric_set_max_events_limit() {
        // Create a MetricSet with a max events limit of 5
        let settings = MetricSetSettings {
            max_events: Some(5),
            max_bytes: None,
            time_to_live: None,
        };
        let mut metric_set = MetricSet::new(settings);
        
        // Push 10 distinct metrics (0-9)
        for i in 0..10 {
            let metric = create_test_metric(
                &format!("test-metric-{}", i),
                MetricKind::Incremental,
                MetricValue::Counter {
                    value: i as f64,
                },);
            metric_set.insert_update(metric);
        }
        
        // Verify we have only 5 metrics in the cache
        assert_eq!(metric_set.len(), 5);
        
        // Verify eviction count is 5
        assert_eq!(metric_set.get_and_reset_eviction_count(), 5);
        
        // Convert to vec and verify we have 5 metrics
        let metrics = metric_set.into_metrics();
        assert_eq!(metrics.len(), 5);
        
        // Print the metrics for debugging
        println!("Metrics after LRU eviction:");
        for (i, metric) in metrics.iter().enumerate() {
            println!("  {}: name={} value={:?}", i, metric.name(), metric.value());
        }
        
        // Collect the metric names - these should be test-metric-5 through test-metric-9
        // since those are the most recently added metrics that should be retained by the LRU cache
        let mut metric_names = Vec::new();
        for metric in &metrics {
            metric_names.push(metric.name().to_string());
        }
        
        // Check that we have the expected metric names (the 5 most recently added)
        for i in 5..10 {
            let expected_name = format!("test-metric-{}", i);
            assert!(
                metric_names.contains(&expected_name),
                "Expected to find metric named {} in result set", expected_name
            );
        }
    }

    #[test]
    fn test_metric_set_max_bytes_limit() {
        // For simplicity, we'll use a small max bytes (enough for ~3 metrics)
        // The exact byte count will depend on implementation details
        let max_bytes = 1000; // Small value for testing
        
        let settings = MetricSetSettings {
            max_events: None,
            max_bytes: Some(max_bytes),
            time_to_live: None,
        };
        let mut metric_set = MetricSet::new(settings);
        
        // Insert metrics until we exceed the max_bytes limit
        for i in 0..10 {
            let metric = create_test_metric(
                &format!("test-metric-{}", i),
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: i as f64,
                },
            );
            metric_set.insert_update(metric);
        }
        
        // Verify memory usage is less than or equal to max_bytes
        let memory_usage = metric_set.weighted_size();
        assert!(memory_usage <= max_bytes as u64,
            "Memory usage {} exceeds max_bytes {}", memory_usage, max_bytes);
        
        // Verify eviction count is positive (exact value depends on implementation)
        let eviction_count = metric_set.get_and_reset_eviction_count();
        assert!(eviction_count > 0, "Expected some evictions due to memory limits");
        
        // Convert to vec and verify the metrics
        let metrics = metric_set.into_metrics();
        
        // Print the metrics for debugging
        println!("Metrics after memory-based eviction:");
        for (i, metric) in metrics.iter().enumerate() {
            println!("  {}: name={} value={:?}", i, metric.name(), metric.value());
        }
        
        // The size of metrics should be less than 10 due to eviction
        assert!(metrics.len() < 10 && metrics.len() > 0,
            "Expected some metrics to be evicted, got {} metrics", metrics.len());
        
        // Check for some of the most recently added metrics (they should be retained by LRU eviction)
        // We can't check for exact indices since memory usage varies, but at least the most recent
        // metrics should be present
        let metric_names: Vec<String> = metrics.iter()
            .map(|m| m.name().to_string())
            .collect();
            
        // Check that at least metric-8 and metric-9 are present (the most recently added)
        let has_recent = metric_names.contains(&"test-metric-9".to_string()) || 
                         metric_names.contains(&"test-metric-8".to_string());
                         
        assert!(has_recent, "Expected at least one of the most recent metrics to be retained");
    }
    //
    // #[test]
    // fn test_incremental_to_absolute_conversion() {
    //     let mut metric_set = MetricSet::default();
    //
    //     // Create a series of incremental counter metrics with the same series
    //     let tags = Some(HashMap::from([("host".to_string(), "test-host".to_string())]));
    //
    //     // Process a sequence of incremental metrics
    //     let incremental1 = create_test_metric("test-metric", MetricKind::Incremental, 1.0, tags.clone());
    //     let absolute1 = metric_set.make_absolute(incremental1.clone()).unwrap();
    //
    //     // First metric should be converted to absolute with the same value
    //     assert_eq!(absolute1.kind(), MetricKind::Absolute);
    //     match absolute1.value() {
    //         MetricValue::Counter { value } => assert_eq!(*value, 1.0),
    //         _ => panic!("Expected counter metric"),
    //     }
    //
    //     // Send a second incremental metric
    //     let incremental2 = create_test_metric("test-metric", MetricKind::Incremental, 2.0, tags.clone());
    //     let absolute2 = metric_set.make_absolute(incremental2.clone()).unwrap();
    //
    //     // Second metric should be converted to absolute with accumulated value (1.0 + 2.0 = 3.0)
    //     assert_eq!(absolute2.kind(), MetricKind::Absolute);
    //     match absolute2.value() {
    //         MetricValue::Counter { value } => assert_eq!(*value, 3.0),
    //         _ => panic!("Expected counter metric"),
    //     }
    //
    //     // Verify gauges are handled correctly
    //     let gauge_metric = create_gauge_metric("test-gauge", MetricKind::Incremental, 5.0, tags.clone());
    //
    //     // Process the gauge metric
    //     let gauge_absolute = metric_set.make_absolute(gauge_metric.clone()).unwrap();
    //
    //     // Gauge should be converted to absolute with the same value
    //     assert_eq!(gauge_absolute.kind(), MetricKind::Absolute);
    //     match gauge_absolute.value() {
    //         MetricValue::Gauge { value } => assert_eq!(*value, 5.0),
    //         _ => panic!("Expected gauge metric"),
    //     }
    // }
    //
    // #[test]
    // fn test_absolute_to_incremental_conversion() {
    //     let mut metric_set = MetricSet::default();
    //
    //     // Create a series of absolute counter metrics with the same series
    //     let tags = Some(HashMap::from([("host".to_string(), "test-host".to_string())]));
    //
    //     // Process a sequence of absolute metrics
    //     let absolute1 = create_test_metric("test-metric", MetricKind::Absolute, 10.0, tags.clone());
    //
    //     // First metric should be stored but not emitted (returns None)
    //     let incremental1 = metric_set.make_incremental(absolute1.clone());
    //     assert!(incremental1.is_none(), "First absolute metric should not produce an incremental output");
    //
    //     // Send a second absolute metric with a higher value
    //     let absolute2 = create_test_metric("test-metric", MetricKind::Absolute, 15.0, tags.clone());
    //     let incremental2 = metric_set.make_incremental(absolute2.clone()).unwrap();
    //
    //     // Second metric should be converted to incremental with the delta (15.0 - 10.0 = 5.0)
    //     assert_eq!(incremental2.kind(), MetricKind::Incremental);
    //     match incremental2.value() {
    //         MetricValue::Counter { value } => assert_eq!(*value, 5.0),
    //         _ => panic!("Expected counter metric"),
    //     }
    //
    //     // Send a third absolute metric with a lower value (simulating counter reset)
    //     let absolute3 = create_test_metric("test-metric", MetricKind::Absolute, 3.0, tags.clone());
    //     let incremental3 = metric_set.make_incremental(absolute3.clone()).unwrap();
    //
    //     // Third metric should produce an incremental metric with the new value
    //     assert_eq!(incremental3.kind(), MetricKind::Incremental);
    //     match incremental3.value() {
    //         MetricValue::Counter { value } => assert_eq!(*value, 3.0),
    //         _ => panic!("Expected counter metric with reset value"),
    //     }
    //
    //     // Verify gauges are handled correctly
    //     let gauge_metric = create_gauge_metric("test-gauge", MetricKind::Absolute, 5.0, tags.clone());
    //
    //     // Process the gauge metric
    //     // First gauge should be stored but not emitted
    //     let gauge_incremental1 = metric_set.make_incremental(gauge_metric.clone());
    //     assert!(gauge_incremental1.is_none(), "First gauge metric should not produce an incremental output");
    //
    //     // Send a second gauge metric
    //     let gauge_metric2 = create_gauge_metric("test-gauge", MetricKind::Absolute, 8.0, tags.clone());
    //
    //     // Process the second gauge metric
    //     let gauge_incremental2 = metric_set.make_incremental(gauge_metric2.clone()).unwrap();
    //
    //     // Gauge should be converted to incremental with the delta (8.0 - 5.0 = 3.0)
    //     assert_eq!(gauge_incremental2.kind(), MetricKind::Incremental);
    //     match gauge_incremental2.value() {
    //         MetricValue::Gauge { value } => assert_eq!(*value, 3.0),
    //         _ => panic!("Expected gauge metric"),
    //     }
    // }
}
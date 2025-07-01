use indexmap::IndexMap;

use std::time::{Duration, Instant};

use vector_lib::event::{
    metric::{MetricData, MetricSeries},
    EventMetadata, Metric, MetricKind,
};

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
    /// Creates a new normalizer with TTL policy.
    pub fn with_ttl(normalizer: N, ttl: TtlPolicy) -> Self {
        Self {
            state: MetricSet::with_ttl_policy(ttl),
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

/// Represents a stored metric entry with its data, metadata, and optional timestamp.
#[derive(Clone, Debug)]
pub struct MetricEntry {
    /// The metric data containing the value and kind
    pub data: MetricData,
    /// Event metadata associated with this metric
    pub metadata: EventMetadata,
    /// Optional timestamp for TTL tracking
    pub timestamp: Option<Instant>,
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

    /// Creates a new MetricEntry from a Metric and optional timestamp.
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
}

/// Configuration for automatic cleanup of expired entries.
#[derive(Clone, Debug)]
pub struct TtlPolicy {
    /// Time-to-live for entries
    pub ttl: Duration,
    /// How often to run cleanup
    pub cleanup_interval: Duration,
    /// Last time cleanup was performed
    pub(crate) last_cleanup: Instant,
}

impl TtlPolicy {
    /// Creates a new cleanup configuration with TTL.
    /// Cleanup interval defaults to TTL/10 with a 10-second minimum.
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            cleanup_interval: ttl.div_f32(10.0).max(Duration::from_secs(10)),
            last_cleanup: Instant::now(),
        }
    }

    /// Checks if it's time to run cleanup.
    pub fn should_cleanup(&self) -> bool {
        Instant::now().duration_since(self.last_cleanup) >= self.cleanup_interval
    }

    /// Marks cleanup as having been performed.
    pub fn mark_cleanup_done(&mut self) {
        self.last_cleanup = Instant::now();
    }
}

/// Metric storage for use with normalization.
///
/// This is primarily a wrapper around [`IndexMap`] (to ensure insertion order
/// is maintained) with convenience methods to make it easier to perform
/// normalization-specific operations. It also includes an optional TTL policy
/// to automatically expire old entries.
#[derive(Clone, Debug, Default)]
pub struct MetricSet {
    inner: IndexMap<MetricSeries, MetricEntry>,
    ttl_policy: Option<TtlPolicy>,
}

impl MetricSet {
    /// Creates an empty MetricSet with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: IndexMap::with_capacity(capacity),
            ttl_policy: None,
        }
    }

    /// Creates a MetricSet with custom cleanup configuration.
    pub fn with_ttl_policy(ttl_policy: TtlPolicy) -> Self {
        Self {
            inner: IndexMap::default(),
            ttl_policy: Some(ttl_policy),
        }
    }

    /// Gets a reference to the TTL policy configuration.
    pub const fn ttl_policy(&self) -> Option<&TtlPolicy> {
        self.ttl_policy.as_ref()
    }

    /// Gets a mutable reference to the TTL policy configuration.
    pub const fn ttl_policy_mut(&mut self) -> Option<&mut TtlPolicy> {
        self.ttl_policy.as_mut()
    }

    /// Perform periodic cleanup if enough time has passed since the last cleanup
    fn maybe_cleanup(&mut self) {
        // Return early if no cleanup is needed
        if !self
            .ttl_policy()
            .is_some_and(|config| config.should_cleanup())
        {
            return;
        }
        self.cleanup_expired();
        if let Some(config) = self.ttl_policy_mut() {
            config.mark_cleanup_done();
        }
    }

    /// Removes expired entries based on TTL if configured.
    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        if let Some(config) = &self.ttl_policy {
            self.inner.retain(|_, entry| match entry.timestamp {
                Some(ts) => now.duration_since(ts) < config.ttl,
                None => true,
            });
        }
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    fn create_timestamp(&self) -> Option<Instant> {
        match self.ttl_policy() {
            Some(_) => Some(Instant::now()),
            _ => None,
        }
    }

    /// Returns true if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Consumes this MetricSet and returns a vector of Metric.
    pub fn into_metrics(mut self) -> Vec<Metric> {
        // Always cleanup on final consumption
        self.cleanup_expired();
        self.inner
            .into_iter()
            .map(|(series, entry)| entry.into_metric(series))
            .collect()
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
        match self.inner.get_mut(metric.series()) {
            Some(existing) => {
                if existing.data.value.add(metric.value()) {
                    metric = metric.with_value(existing.data.value.clone());
                    existing.update_timestamp(timestamp);
                } else {
                    // Metric changed type, store this as the new reference value
                    let (series, entry) = MetricEntry::from_metric(metric.clone(), timestamp);
                    self.inner.insert(series, entry);
                }
            }
            None => {
                let (series, entry) = MetricEntry::from_metric(metric.clone(), timestamp);
                self.inner.insert(series, entry);
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
        match self.inner.get_mut(metric.series()) {
            Some(reference) => {
                let new_value = metric.value().clone();
                // From the stored reference value, emit an increment
                if metric.subtract(&reference.data) {
                    reference.data.value = new_value;
                    reference.update_timestamp(timestamp);
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
        self.inner.insert(series, entry);
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
                        let (series, data, metadata) = metric.into_parts();
                        if existing.data.update(&data) {
                            existing.metadata.merge(metadata);
                            existing.update_timestamp(timestamp);
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

    /// Removes a series from the set.
    ///
    /// If the series existed and was removed, returns `true`.  Otherwise, `false`.
    pub fn remove(&mut self, series: &MetricSeries) -> bool {
        self.maybe_cleanup();
        self.inner.shift_remove(series).is_some()
    }
}

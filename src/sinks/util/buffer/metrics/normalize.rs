use mini_moka::sync::Cache;
use std::marker::PhantomData;
use std::time::Duration;

use vector_lib::event::{
    metric::{MetricData, MetricSeries},
    EventMetadata, Metric, MetricKind,
};

use metrics::gauge;
use serde_with::serde_as;
use snafu::Snafu;
use vector_common::internal_event::InternalEvent;
use vector_config_macros::configurable_component;
use vector_lib::ByteSizeOf;

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum NormalizerError {
    #[snafu(display("`max_bytes` must be greater than zero"))]
    InvalidMaxBytes,
    #[snafu(display("`max_events` must be greater than zero"))]
    InvalidMaxEvents,
    #[snafu(display("`time_to_idle` must be greater than zero"))]
    InvalidTimeToIdle,
    #[snafu(display("cannot specify both max_bytes and max_events"))]
    ConflictingLimits,
}

/// Defines behavior for creating the MetricNormalizer. Note that the mini-moka LRU cache
/// supports either `max_events` or `max_bytes` (or neither), but not both.
#[serde_as]
#[configurable_component]
#[configurable(metadata(docs::advanced))]
#[derive(Clone, Copy, Debug, Default)]
pub struct NormalizerConfig<D: NormalizerSettings + Clone> {
    /// The maximum size in bytes of the metrics normalizer cache.
    /// Either `max_bytes` or `max_events` can be specified, or neither, but not both.
    #[serde(default = "default_max_bytes::<D>")]
    #[configurable(metadata(docs::type_unit = "bytes"))]
    pub max_bytes: Option<usize>,

    /// The maximum number of events of the metrics normalizer cache.
    /// Either `max_bytes` or `max_events` can be specified, or neither, but not both.
    #[serde(default = "default_max_events::<D>")]
    #[configurable(metadata(docs::type_unit = "events"))]
    pub max_events: Option<usize>,

    /// The maximum age of a metric not being updated before it is evicted from the metrics normalizer cache.
    #[serde(default = "default_time_to_idle::<D>")]
    #[configurable(metadata(docs::type_unit = "seconds"))]
    #[configurable(metadata(docs::human_name = "Time To Idle"))]
    pub time_to_idle: Option<u64>,

    #[serde(skip)]
    pub _d: PhantomData<D>,
}

const fn default_max_bytes<D: NormalizerSettings>() -> Option<usize> {
    D::MAX_BYTES
}

const fn default_max_events<D: NormalizerSettings>() -> Option<usize> {
    D::MAX_EVENTS
}

const fn default_time_to_idle<D: NormalizerSettings>() -> Option<u64> {
    D::TIME_TO_IDLE
}

impl<D: NormalizerSettings + Clone> NormalizerConfig<D> {
    pub fn validate(&self) -> Result<NormalizerConfig<D>, NormalizerError> {
        let config = NormalizerConfig::<D> {
            max_bytes: self.max_bytes.or(D::MAX_BYTES),
            max_events: self.max_events.or(D::MAX_EVENTS),
            time_to_idle: self.time_to_idle.or(D::TIME_TO_IDLE),
            _d: Default::default(),
        };
        match (config.max_bytes, config.max_events, config.time_to_idle) {
            (Some(0), _, _) => Err(NormalizerError::InvalidMaxBytes),
            (_, Some(0), _) => Err(NormalizerError::InvalidMaxEvents),
            (_, _, Some(timeout)) if timeout <= 0 => Err(NormalizerError::InvalidTimeToIdle),
            (Some(_), Some(_), _) => Err(NormalizerError::ConflictingLimits),
            _ => Ok(config),
        }
    }

    pub fn into_settings(self) -> MetricSetSettings {
        MetricSetSettings {
            max_bytes: self.max_bytes,
            max_events: self.max_events,
            time_to_idle: self.time_to_idle,
        }
    }
}

pub trait NormalizerSettings {
    const MAX_EVENTS: Option<usize>;
    const MAX_BYTES: Option<usize>;
    const TIME_TO_IDLE: Option<u64>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultNormalizerSettings;

impl NormalizerSettings for DefaultNormalizerSettings {
    const MAX_EVENTS: Option<usize> = None;
    const MAX_BYTES: Option<usize> = None;
    const TIME_TO_IDLE: Option<u64> = None;
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
            .unwrap_or_else(|e| panic!("Invalid cache settings: {:?}", e))
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

/// Represents a stored metric entry with its data and metadata.
#[derive(Clone, Debug)]
pub struct MetricEntry {
    /// The metric data containing the value and kind
    pub data: MetricData,
    /// Event metadata associated with this metric
    pub metadata: EventMetadata,
}

impl ByteSizeOf for MetricEntry {
    fn allocated_bytes(&self) -> usize {
        self.data.allocated_bytes() + self.metadata.allocated_bytes()
    }
}

impl MetricEntry {
    /// Creates a new MetricEntry with the given data and metadata.
    pub const fn new(data: MetricData, metadata: EventMetadata) -> Self {
        Self { data, metadata }
    }

    /// Creates a new MetricEntry from a Metric.
    pub fn from_metric(metric: Metric) -> (MetricSeries, Self) {
        let (series, data, metadata) = metric.into_parts();
        let entry = Self::new(data, metadata);
        (series, entry)
    }

    /// Converts this entry back to a Metric with the given series.
    pub fn into_metric(self, series: MetricSeries) -> Metric {
        Metric::from_parts(series, self.data, self.metadata)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MetricSetSettings {
    pub max_bytes: Option<usize>,
    pub max_events: Option<usize>,
    pub time_to_idle: Option<u64>,
}

impl Default for MetricSetSettings {
    fn default() -> Self {
        Self {
            max_events: None,
            max_bytes: None,
            time_to_idle: None,
        }
    }
}

/// LRU cache using Mini-Moka with optional eviction based on (optionally) TTI or (optionally)
/// either memory usage OR entry count, but not both
#[derive(Clone, Debug)]
pub struct MetricSet {
    /// Mini-Moka LRU cache configured with either memory-based or count-based eviction
    cache: Cache<MetricSeries, MetricEntry>,
    /// Current configuration for debugging/monitoring
    settings: MetricSetSettings,
}

impl MetricSet {
    /// Prints the configuration and contents of the cache for debugging purposes.
    pub fn debug_print(&self) {
        // Print configuration
        println!("MetricSet Configuration:");
        println!("  Max Bytes: {:?}", self.settings.max_bytes);
        println!("  Max Events: {:?}", self.settings.max_events);
        println!("  Time To Idle: {:?}", self.settings.time_to_idle);
        println!("  Entry Count: {}", self.cache.entry_count());
        println!("  Weighted Size: {} bytes", self.cache.weighted_size());

        // Print cache contents
        println!("\nMetricSet Cache Contents:");
        for (i, (series, entry)) in self.iter().enumerate() {
            println!(
                "  Item {}: Series: {:?}, Value: {:?}, Kind: {:?}",
                i, series, entry.data.value, entry.data.kind
            );
        }
    }

    /// Creates a new MetricSet with the given configuration.
    pub fn new(settings: MetricSetSettings) -> Self {
        let mut builder = Cache::builder();

        // Configure either memory-based OR count-based eviction, but not both (not supported by moka).
        match (settings.max_bytes, settings.max_events) {
            (Some(max_bytes), None) => {
                // Memory-based eviction using weigher
                builder = builder.max_capacity(max_bytes as u64).weigher(
                    |series: &MetricSeries, entry: &MetricEntry| -> u32 {
                        (series.allocated_bytes() + entry.allocated_bytes()) as u32
                    },
                );
            }
            (None, Some(max_events)) => {
                // Entry count-based eviction
                builder = builder.max_capacity(max_events as u64);
            }
            (None, None) => {
                // No capacity limit - unlimited cache
            }
            (Some(_), Some(_)) => {
                // This should be caught during validation
                panic!("Cannot specify both max_bytes and max_events");
            }
        }

        // Set TTI if specified
        if let Some(tti) = settings.time_to_idle {
            builder = builder.time_to_idle(Duration::from_secs(tti));
        }

        let cache = builder.build();

        Self { cache, settings }
    }

    /// Gets the current configuration.
    pub const fn config(&self) -> &MetricSetSettings {
        &self.settings
    }

    /// Gets the current number of entries in the cache.
    /// Note: This may include entries that have expired but haven't been cleaned up yet.
    pub fn len(&self) -> usize {
        self.cache.entry_count() as usize
    }

    /// Returns true if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.cache.entry_count() == 0
    }

    /// Gets the current weighted size of all entries.
    /// This represents the estimated memory usage in bytes.
    pub fn weighted_size(&self) -> u64 {
        self.cache.weighted_size()
    }

    /// Consumes this MetricSet and returns a vector of Metric.
    pub fn into_metrics(self) -> Vec<Metric> {
        let mut metrics = Vec::new();
        for entry_ref in self.cache.iter() {
            let (series, entry) = entry_ref.pair();
            metrics.push(entry.clone().into_metric(series.clone()));
        }
        metrics
    }

    /// Either pass the metric through as-is if absolute, or convert it
    /// to absolute if incremental.
    pub fn make_absolute(&mut self, metric: Metric) -> Option<Metric> {
        match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => Some(self.incremental_to_absolute(metric)),
        }
    }

    /// Either convert the metric to incremental if absolute, or
    /// aggregate it with any previous value if already incremental.
    pub fn make_incremental(&mut self, metric: Metric) -> Option<Metric> {
        match metric.kind() {
            MetricKind::Absolute => self.absolute_to_incremental(metric),
            MetricKind::Incremental => Some(metric),
        }
    }

    /// Convert the incremental metric into an absolute one, using the
    /// state buffer to keep track of the value throughout the entire
    /// application uptime.
    fn incremental_to_absolute(&mut self, mut metric: Metric) -> Metric {
        self.debug_print();
        let series = metric.series().clone();

        if let Some(mut existing) = self.cache.get(&series) {
            if existing.data.value.add(metric.value()) {
                metric = metric.with_value(existing.data.value.clone());
                // Update the cached entry with the new accumulated value
                let (_, updated_entry) = MetricEntry::from_metric(metric.clone());
                self.cache.insert(series, updated_entry);
            } else {
                // Metric changed type, store this as the new reference value
                self.insert(metric.clone());
            }
        } else {
            // First time seeing this metric, store it
            self.insert(metric.clone());
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
        let series = metric.series().clone();

        if let Some(mut reference) = self.cache.get(&series) {
            let new_value = metric.value().clone();
            // From the stored reference value, emit an increment
            if metric.subtract(&reference.data) {
                // Update the reference with the new absolute value
                reference.data.value = new_value;
                self.cache.insert(series, reference);
                Some(metric.into_incremental())
            } else {
                // Metric changed type, store this and emit nothing
                self.insert(metric);
                None
            }
        } else {
            // No reference so store this and emit nothing
            self.insert(metric);
            None
        }
    }

    fn insert(&mut self, metric: Metric) {
        let (series, entry) = MetricEntry::from_metric(metric);
        self.cache.insert(series, entry);
    }

    pub fn insert_update(&mut self, metric: Metric) {
        let series = metric.series().clone();

        let update = match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => {
                // Incremental metrics update existing entries, if present
                if let Some(mut existing) = self.cache.get(&series) {
                    let (series, data, metadata) = metric.into_parts();
                    if existing.data.update(&data) {
                        existing.metadata.merge(metadata);
                        self.cache.insert(series, existing);
                        None
                    } else {
                        warn!(message = "Metric changed type, dropping old value.", %series);
                        Some(Metric::from_parts(series, data, metadata))
                    }
                } else {
                    Some(metric)
                }
            }
        };
        if let Some(metric) = update {
            self.insert(metric);
        }
    }

    /// Removes a series from the cache.
    ///
    /// If the series existed and was removed, returns true.  Otherwise, false.
    pub fn remove(&mut self, series: &MetricSeries) {
        self.cache.invalidate(series)
    }

    /// Returns true if the cache contains the specified series (and it hasn't expired).
    pub fn contains(&self, series: &MetricSeries) -> bool {
        self.cache.contains_key(series)
    }

    /// Gets a clone of the entry for the given series, if it exists and hasn't expired.
    pub fn get(&self, series: &MetricSeries) -> Option<MetricEntry> {
        self.cache.get(series)
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Returns an iterator over all cached entries.
    /// This is useful for debugging or monitoring cache contents.
    pub fn iter(&self) -> impl Iterator<Item = (MetricSeries, MetricEntry)> + '_ {
        self.cache.iter().map(|entry_ref| {
            let (series, entry) = entry_ref.pair();
            (series.clone(), entry.clone())
        })
    }
}

impl Default for MetricSet {
    fn default() -> Self {
        Self::new(MetricSetSettings::default())
    }
}

impl InternalEvent for MetricSet {
    #[allow(clippy::cast_precision_loss)]
    fn emit(self) {
        if self.cache.entry_count() != 0 {
            gauge!("cache_events").set(self.cache.entry_count() as f64);
        }
        if self.cache.weighted_size() != 0 {
            gauge!("cache_bytes_size").set(self.cache.weighted_size() as f64);
        }
    }
}

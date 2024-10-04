use indexmap::IndexMap;

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
    /// Gets a mutable reference to the current metric state for this normalizer.
    pub fn get_state_mut(&mut self) -> &mut MetricSet {
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

type MetricEntry = (MetricData, EventMetadata);

/// Metric storage for use with normalization.
///
/// This is primarily a wrapper around [`IndexMap`] (to ensure insertion order
/// is maintained) with convenience methods to make it easier to perform
/// normalization-specific operations.
#[derive(Clone, Default, Debug)]
pub struct MetricSet(IndexMap<MetricSeries, MetricEntry>);

impl MetricSet {
    /// Creates an empty `MetricSet` with the specified capacity.
    ///
    /// The metric set will be able to hold at least `capacity` elements without reallocating. If `capacity` is 0, the
    /// metric set will not allocate.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(IndexMap::with_capacity(capacity))
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes this `MetricSet` and returns a vector of `Metric`.
    pub fn into_metrics(self) -> Vec<Metric> {
        self.0
            .into_iter()
            .map(|(series, (data, metadata))| Metric::from_parts(series, data, metadata))
            .collect()
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
        match self.0.get_mut(metric.series()) {
            Some(existing) => {
                if existing.0.value.add(metric.value()) {
                    metric = metric.with_value(existing.0.value.clone());
                } else {
                    // Metric changed type, store this as the new reference value
                    self.0.insert(
                        metric.series().clone(),
                        (metric.data().clone(), EventMetadata::default()),
                    );
                }
            }
            None => {
                self.0.insert(
                    metric.series().clone(),
                    (metric.data().clone(), EventMetadata::default()),
                );
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
        match self.0.get_mut(metric.series()) {
            Some(reference) => {
                let new_value = metric.value().clone();
                // From the stored reference value, emit an increment
                if metric.subtract(&reference.0) {
                    reference.0.value = new_value;
                    Some(metric.into_incremental())
                } else {
                    // Metric changed type, store this and emit nothing
                    self.insert(metric);
                    None
                }
            }
            None => {
                // No reference so store this and emit nothing
                self.insert(metric);
                None
            }
        }
    }

    fn insert(&mut self, metric: Metric) {
        let (series, data, metadata) = metric.into_parts();
        self.0.insert(series, (data, metadata));
    }

    pub fn insert_update(&mut self, metric: Metric) {
        let update = match metric.kind() {
            MetricKind::Absolute => Some(metric),
            MetricKind::Incremental => {
                // Incremental metrics update existing entries, if present
                match self.0.get_mut(metric.series()) {
                    Some(existing) => {
                        let (series, data, metadata) = metric.into_parts();
                        if existing.0.update(&data) {
                            existing.1.merge(metadata);
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
            self.insert(metric);
        }
    }

    /// Removes a series from the set.
    ///
    /// If the series existed and was removed, returns `true`.  Otherwise, `false`.
    pub fn remove(&mut self, series: &MetricSeries) -> bool {
        self.0.shift_remove(series).is_some()
    }
}

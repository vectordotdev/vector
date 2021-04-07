use crate::metrics::registry::VectorRegistry;
use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::{CompositeKey, Handle, MetricKind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
pub(crate) struct VectorRecorder {
    registry: VectorRegistry<CompositeKey, Handle>,
    cardinality_counter: Arc<AtomicU64>,
}

impl VectorRecorder {
    pub fn new(
        registry: VectorRegistry<CompositeKey, Handle>,
        cardinality_counter: Arc<AtomicU64>,
    ) -> Self {
        Self {
            registry,
            cardinality_counter,
        }
    }
}

impl VectorRecorder {
    fn bump_cardinality_counter_and<F, O>(&self, f: F) -> O
    where
        F: FnOnce() -> O,
    {
        self.cardinality_counter.fetch_add(1, Ordering::Relaxed);
        f()
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::counter),
        );
    }

    fn register_gauge(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::gauge),
        );
    }

    fn register_histogram(
        &self,
        key: Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(
            ckey,
            |_| {},
            || self.bump_cardinality_counter_and(Handle::histogram),
        )
    }

    fn increment_counter(&self, key: Key, value: u64) {
        let ckey = CompositeKey::new(MetricKind::Counter, key);
        self.registry.op(
            ckey,
            |handle| handle.increment_counter(value),
            || self.bump_cardinality_counter_and(Handle::counter),
        );
    }

    fn update_gauge(&self, key: Key, value: GaugeValue) {
        let ckey = CompositeKey::new(MetricKind::Gauge, key);
        self.registry.op(
            ckey,
            |handle| handle.update_gauge(value),
            || self.bump_cardinality_counter_and(Handle::gauge),
        );
    }

    fn record_histogram(&self, key: Key, value: f64) {
        let ckey = CompositeKey::new(MetricKind::Histogram, key);
        self.registry.op(
            ckey,
            |handle| handle.record_histogram(value),
            || self.bump_cardinality_counter_and(Handle::histogram),
        );
    }
}

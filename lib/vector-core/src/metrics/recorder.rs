use crate::metrics::handle::Handle;
use crate::metrics::registry::VectorRegistry;
use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::{CompositeKey, MetricKind};

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
pub(crate) struct VectorRecorder {
    registry: VectorRegistry<CompositeKey, Handle>,
}

impl VectorRecorder {
    pub fn new(registry: VectorRegistry<CompositeKey, Handle>) -> Self {
        Self { registry }
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let composite_key = CompositeKey::new(MetricKind::Counter, key.to_owned());
        self.registry.op(composite_key, |_| {}, Handle::counter);
    }

    fn register_gauge(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        let composite_key = CompositeKey::new(MetricKind::Gauge, key.to_owned());
        self.registry.op(composite_key, |_| {}, Handle::gauge);
    }

    fn register_histogram(
        &self,
        key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        let composite_key = CompositeKey::new(MetricKind::Histogram, key.to_owned());
        self.registry.op(composite_key, |_| {}, Handle::histogram)
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        let composite_key = CompositeKey::new(MetricKind::Counter, key.to_owned());
        self.registry.op(
            composite_key,
            |handle| handle.increment_counter(value),
            Handle::counter,
        );
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        let composite_key = CompositeKey::new(MetricKind::Gauge, key.to_owned());
        self.registry.op(
            composite_key,
            |handle| handle.update_gauge(value),
            Handle::gauge,
        );
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        let composite_key = CompositeKey::new(MetricKind::Histogram, key.to_owned());
        self.registry.op(
            composite_key,
            |handle| handle.record_histogram(value),
            Handle::histogram,
        );
    }
}

use std::sync::Arc;

use crate::metrics::handle::Handle;
use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::{MetricKind, NotTracked, Registry};

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
pub(crate) struct VectorRecorder {
    registry: Arc<Registry<Key, Handle, NotTracked<Handle>>>,
}

impl VectorRecorder {
    pub fn new(registry: Arc<Registry<Key, Handle, NotTracked<Handle>>>) -> Self {
        Self { registry }
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.registry
            .op(MetricKind::Counter, key, |_| {}, Handle::counter);
    }

    fn register_gauge(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.registry
            .op(MetricKind::Gauge, key, |_| {}, Handle::gauge);
    }

    fn register_histogram(
        &self,
        key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        self.registry
            .op(MetricKind::Histogram, key, |_| {}, Handle::histogram);
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        self.registry.op(
            MetricKind::Counter,
            key,
            |handle| handle.increment_counter(value),
            Handle::counter,
        );
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.registry.op(
            MetricKind::Gauge,
            key,
            |handle| handle.update_gauge(value),
            Handle::gauge,
        );
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.registry.op(
            MetricKind::Histogram,
            key,
            |handle| handle.record_histogram(value),
            Handle::histogram,
        );
    }
}

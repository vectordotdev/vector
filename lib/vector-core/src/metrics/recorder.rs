use std::sync::Arc;

use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::MetricKind;
use once_cell::unsync::OnceCell;

use super::Registry;
use crate::metrics::handle::Handle;

thread_local!(static LOCAL_REGISTRY: OnceCell<Registry>=OnceCell::new());

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
#[derive(Clone)]
pub(super) enum VectorRecorder {
    Global(Arc<Registry>),
    ThreadLocal,
}

impl VectorRecorder {
    pub(super) fn new_global() -> Self {
        let registry = Arc::new(Registry::untracked());
        Self::Global(registry)
    }

    pub(super) fn new_test() -> Self {
        Self::with_thread_local(Registry::clear);
        Self::ThreadLocal
    }

    pub(super) fn with_registry<T>(&self, doit: impl FnOnce(&Registry) -> T) -> T {
        match &self {
            Self::Global(registry) => doit(registry),
            Self::ThreadLocal => Self::with_thread_local(doit),
        }
    }

    fn with_thread_local<T>(doit: impl FnOnce(&Registry) -> T) -> T {
        LOCAL_REGISTRY.with(|oc| doit(oc.get_or_init(Registry::untracked)))
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.with_registry(|r| r.op(MetricKind::Counter, key, |_| {}, Handle::counter));
    }

    fn register_gauge(&self, key: &Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.with_registry(|r| r.op(MetricKind::Gauge, key, |_| {}, Handle::gauge));
    }

    fn register_histogram(
        &self,
        key: &Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        self.with_registry(|r| r.op(MetricKind::Histogram, key, |_| {}, Handle::histogram));
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        self.with_registry(|r| {
            r.op(
                MetricKind::Counter,
                key,
                |handle| handle.increment_counter(value),
                Handle::counter,
            );
        });
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        self.with_registry(|r| {
            r.op(
                MetricKind::Gauge,
                key,
                |handle| handle.update_gauge(value),
                Handle::gauge,
            );
        });
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        self.with_registry(|r| {
            r.op(
                MetricKind::Histogram,
                key,
                |handle| handle.record_histogram(value),
                Handle::histogram,
            );
        });
    }
}

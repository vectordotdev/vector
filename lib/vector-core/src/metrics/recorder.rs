use std::sync::Arc;

use metrics::{Counter, Gauge, Histogram, Key, KeyName, Recorder, Unit};
use once_cell::unsync::OnceCell;

use super::storage::VectorStorage;

pub(super) type Registry = metrics_util::registry::Registry<Key, VectorStorage>;

thread_local!(static LOCAL_REGISTRY: OnceCell<Registry> = OnceCell::new());

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
#[derive(Clone)]
pub(super) enum VectorRecorder {
    Global(Arc<Registry>),
    ThreadLocal,
}

impl VectorRecorder {
    pub(super) fn new_global() -> Self {
        let registry = Arc::new(Registry::new(VectorStorage));
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
        LOCAL_REGISTRY.with(|oc| doit(oc.get_or_init(|| Registry::new(VectorStorage))))
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key) -> Counter {
        self.with_registry(|r| r.get_or_create_counter(key, |c| c.clone().into()))
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        self.with_registry(|r| r.get_or_create_gauge(key, |g| g.clone().into()))
    }

    fn register_histogram(&self, key: &Key) -> Histogram {
        self.with_registry(|r| r.get_or_create_histogram(key, |h| h.clone().into()))
    }

    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: &'static str) {}

    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: &'static str) {}

    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: &'static str) {}
}

use std::sync::{atomic::Ordering, Arc, RwLock};
use std::{cell::OnceCell, time::Duration};

use chrono::Utc;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use metrics_util::{registry::Registry as MetricsRegistry, MetricKindMask};
use quanta::Clock;

use super::recency::{GenerationalStorage, Recency};
use super::storage::VectorStorage;
use crate::event::{Metric, MetricValue};

thread_local!(static LOCAL_REGISTRY: OnceCell<Registry> = const { OnceCell::new() });

#[allow(dead_code)]
pub(super) struct Registry {
    registry: MetricsRegistry<Key, GenerationalStorage<VectorStorage>>,
    recency: RwLock<Option<Recency<Key>>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            registry: MetricsRegistry::new(GenerationalStorage::new(VectorStorage)),
            recency: RwLock::new(None),
        }
    }

    pub(super) fn clear(&self) {
        self.registry.clear();
    }

    pub(super) fn set_expiry(&self, timeout: Option<Duration>) {
        let recency = timeout.map(|_| Recency::new(Clock::new(), MetricKindMask::ALL, timeout));
        *(self.recency.write()).expect("Failed to acquire write lock on recency map") = recency;
    }

    pub(super) fn visit_metrics(&self) -> Vec<Metric> {
        let timestamp = Utc::now();

        let mut metrics = Vec::new();
        let recency = self
            .recency
            .read()
            .expect("Failed to acquire read lock on recency map");
        let recency = recency.as_ref();

        for (key, counter) in self.registry.get_counter_handles() {
            if recency.map_or(true, |recency| {
                recency.should_store_counter(&key, &counter, &self.registry)
            }) {
                // NOTE this will truncate if the value is greater than 2**52.
                #[allow(clippy::cast_precision_loss)]
                let value = counter.get_inner().load(Ordering::Relaxed) as f64;
                let value = MetricValue::Counter { value };
                metrics.push(Metric::from_metric_kv(&key, value, timestamp));
            }
        }
        for (key, gauge) in self.registry.get_gauge_handles() {
            if recency.map_or(true, |recency| {
                recency.should_store_gauge(&key, &gauge, &self.registry)
            }) {
                let value = gauge.get_inner().load(Ordering::Relaxed);
                let value = MetricValue::Gauge { value };
                metrics.push(Metric::from_metric_kv(&key, value, timestamp));
            }
        }
        for (key, histogram) in self.registry.get_histogram_handles() {
            if recency.map_or(true, |recency| {
                recency.should_store_histogram(&key, &histogram, &self.registry)
            }) {
                let value = histogram.get_inner().make_metric();
                metrics.push(Metric::from_metric_kv(&key, value, timestamp));
            }
        }
        metrics
    }

    fn get_counter(&self, key: &Key) -> Counter {
        self.registry
            .get_or_create_counter(key, |c| c.clone().into())
    }

    fn get_gauge(&self, key: &Key) -> Gauge {
        self.registry.get_or_create_gauge(key, |c| c.clone().into())
    }

    fn get_histogram(&self, key: &Key) -> Histogram {
        self.registry
            .get_or_create_histogram(key, |c| c.clone().into())
    }
}

/// [`VectorRecorder`] is a [`metrics::Recorder`] implementation that's suitable
/// for the advanced usage that we have in Vector.
///
/// TODO: The latest version of the `metrics` crate has a test recorder interface that could be used
/// to replace this whole global/local switching mechanism, as it effectively does the exact same
/// thing internally. However, it is only available through a `with_test_recorder` function that
/// takes a closure and cleans up the test recorder once the closure finishes. This is a much
/// cleaner interface, but interacts poorly with async code as used by the component tests. The best
/// path forward to make async tests work, then, is to replace the standard `#[tokio::test]` proc
/// macro wrapper with an alternate wrapper that does the normal tokio setup from within the
/// `with_test_recorder` closure, and use it across all the tests that require a test
/// recorder. Given the large number of such tests, we are retaining this global test recorder hack
/// here, but some day we should refactor the tests to eliminate it.
#[derive(Clone)]
pub(super) enum VectorRecorder {
    Global(Arc<Registry>),
    ThreadLocal,
}

impl VectorRecorder {
    pub(super) fn new_global() -> Self {
        Self::Global(Arc::new(Registry::new()))
    }

    pub(super) fn new_test() -> Self {
        Self::with_thread_local(Registry::clear);
        Self::ThreadLocal
    }

    pub(super) fn with_registry<T>(&self, doit: impl FnOnce(&Registry) -> T) -> T {
        match &self {
            Self::Global(registry) => doit(registry),
            // This is only called after the registry is created, so we can just use a dummy
            // idle_timeout parameter.
            Self::ThreadLocal => Self::with_thread_local(doit),
        }
    }

    fn with_thread_local<T>(doit: impl FnOnce(&Registry) -> T) -> T {
        LOCAL_REGISTRY.with(|oc| doit(oc.get_or_init(Registry::new)))
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        self.with_registry(|r| r.get_counter(key))
    }

    fn register_gauge(&self, key: &Key, _: &Metadata<'_>) -> Gauge {
        self.with_registry(|r| r.get_gauge(key))
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        self.with_registry(|r| r.get_histogram(key))
    }

    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
}

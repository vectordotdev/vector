use std::sync::{atomic::Ordering, Arc, RwLock};
use std::time::Duration;

use chrono::Utc;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use metrics_util::{registry::Registry as MetricsRegistry, MetricKindMask};
use quanta::Clock;

use super::recency::{GenerationalStorage, Recency};
use super::storage::VectorStorage;
use crate::event::{Metric, MetricValue};

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
#[derive(Clone)]
pub(super) struct VectorRecorder(Arc<Registry>);

impl VectorRecorder {
    pub(super) fn new() -> Self {
        Self(Arc::new(Registry::new()))
    }

    pub(super) fn registry(&self) -> &Registry {
        &self.0
    }
}

impl Recorder for VectorRecorder {
    fn register_counter(&self, key: &Key, _meta: &Metadata<'_>) -> Counter {
        self.0.get_counter(key)
    }

    fn register_gauge(&self, key: &Key, _meta: &Metadata<'_>) -> Gauge {
        self.0.get_gauge(key)
    }

    fn register_histogram(&self, key: &Key, _meta: &Metadata<'_>) -> Histogram {
        self.0.get_histogram(key)
    }

    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
}

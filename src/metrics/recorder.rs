use crate::metrics::registry::VectorRegistry;
use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::{CompositeKey, Handle, MetricKind};
use std::sync::mpsc;

pub(crate) enum Recording {
    RegisterCounter(Key),
    RegisterGauge(Key),
    RegisterHistogram(Key),
    IncrementCounter(Key, u64),
    UpdateGauge(Key, GaugeValue),
    RecordHistogram(Key, f64),
}

/// [`OuterRecorder`] is a [`metrics::Recorder`] implementation that receives
/// inbound recordings and batches them up for actual recording by the
/// [`InnerRecorder`].
pub(crate) struct OuterRecorder {
    chan: mpsc::Sender<Recording>,
}

impl OuterRecorder {
    pub(crate) fn new(chan: mpsc::Sender<Recording>) -> Self {
        Self { chan }
    }
}

impl Recorder for OuterRecorder {
    fn register_counter(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.chan
            .send(Recording::RegisterCounter(key))
            .expect("receiver hung up");
    }

    fn register_gauge(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.chan
            .send(Recording::RegisterGauge(key))
            .expect("receiver hung up");
    }

    fn register_histogram(
        &self,
        key: Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        self.chan
            .send(Recording::RegisterHistogram(key))
            .expect("receiver hung up");
    }

    fn increment_counter(&self, key: Key, value: u64) {
        self.chan
            .send(Recording::IncrementCounter(key, value))
            .expect("receiver hung up");
    }

    fn update_gauge(&self, key: Key, value: GaugeValue) {
        self.chan
            .send(Recording::UpdateGauge(key, value))
            .expect("receiver hung up");
    }

    fn record_histogram(&self, key: Key, value: f64) {
        self.chan
            .send(Recording::RecordHistogram(key, value))
            .expect("receiver hung up");
    }
}

pub(crate) struct InnerRecorder {
    chan: mpsc::Receiver<Recording>,
    registry: VectorRegistry<CompositeKey, Handle>,
}

impl InnerRecorder {
    pub(crate) fn new(
        chan: mpsc::Receiver<Recording>,
        registry: VectorRegistry<CompositeKey, Handle>,
    ) -> Self {
        Self { chan, registry }
    }

    pub(crate) fn run(self) {
        while let Ok(recording) = self.chan.recv() {
            use Recording::*;
            let mut map = self.registry.map.lock().expect("metrics map poisoned");
            match recording {
                RegisterCounter(key) => {
                    let ckey = CompositeKey::new(MetricKind::COUNTER, key);
                    map.entry(ckey).or_insert_with(|| Handle::counter());
                }
                RegisterGauge(key) => {
                    let ckey = CompositeKey::new(MetricKind::GAUGE, key);
                    map.entry(ckey).or_insert_with(|| Handle::gauge());
                }
                RegisterHistogram(key) => {
                    let ckey = CompositeKey::new(MetricKind::HISTOGRAM, key);
                    map.entry(ckey).or_insert_with(|| Handle::histogram());
                }
                IncrementCounter(key, value) => {
                    let ckey = CompositeKey::new(MetricKind::COUNTER, key);
                    let counter = map.entry(ckey).or_insert_with(|| Handle::counter());
                    counter.increment_counter(value);
                }
                UpdateGauge(key, value) => {
                    let ckey = CompositeKey::new(MetricKind::GAUGE, key);
                    let gauge = map.entry(ckey).or_insert_with(|| Handle::gauge());
                    gauge.update_gauge(value);
                }
                RecordHistogram(key, value) => {
                    let ckey = CompositeKey::new(MetricKind::HISTOGRAM, key);
                    let histogram = map.entry(ckey).or_insert_with(|| Handle::histogram());
                    histogram.record_histogram(value);
                }
            }
        }
    }
}

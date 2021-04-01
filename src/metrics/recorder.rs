use crate::metrics::registry::VectorRegistry;
use metrics::{GaugeValue, Key, Recorder, Unit};
use metrics_util::{CompositeKey, Handle, MetricKind};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug)]
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
    chan: mpsc::SyncSender<Recording>,
}

impl OuterRecorder {
    pub(crate) fn new(chan: mpsc::SyncSender<Recording>) -> Self {
        Self { chan }
    }

    fn send(&self, recording: Recording) {
        self.chan.send(recording).expect("receiver hung up");
    }
}

impl Recorder for OuterRecorder {
    fn register_counter(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.send(Recording::RegisterCounter(key));
    }

    fn register_gauge(&self, key: Key, _unit: Option<Unit>, _description: Option<&'static str>) {
        self.send(Recording::RegisterGauge(key));
    }

    fn register_histogram(
        &self,
        key: Key,
        _unit: Option<Unit>,
        _description: Option<&'static str>,
    ) {
        self.send(Recording::RegisterHistogram(key));
    }

    fn increment_counter(&self, key: Key, value: u64) {
        self.send(Recording::IncrementCounter(key, value));
    }

    fn update_gauge(&self, key: Key, value: GaugeValue) {
        self.send(Recording::UpdateGauge(key, value));
    }

    fn record_histogram(&self, key: Key, value: f64) {
        self.send(Recording::RecordHistogram(key, value));
    }
}

pub(crate) struct InnerRecorder {
    chan: mpsc::Receiver<Recording>,
    registry: VectorRegistry<CompositeKey, Handle>,
}

fn populate<'a>(buffer: &mut Vec<Recording>, registry: &mut VectorRegistry<CompositeKey, Handle>) {
    use Recording::*;
    let mut map = registry.map.lock().expect("metrics map poisoned");
    for recording in buffer.drain(..) {
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

impl InnerRecorder {
    pub(crate) fn new(
        chan: mpsc::Receiver<Recording>,
        registry: VectorRegistry<CompositeKey, Handle>,
    ) -> Self {
        Self { chan, registry }
    }

    pub(crate) fn run(mut self) {
        let max_buffer = u16::MAX as usize / 4;
        let mut buffer = Vec::with_capacity(max_buffer);
        loop {
            if buffer.len() >= max_buffer {
                populate(&mut buffer, &mut self.registry);
            }

            let resp = self.chan.recv_timeout(Duration::from_secs(10));
            match resp {
                Err(mpsc::RecvTimeoutError::Disconnected) => unreachable!(),
                Err(mpsc::RecvTimeoutError::Timeout) => populate(&mut buffer, &mut self.registry),
                Ok(recording) => buffer.push(recording),
            }
        }
    }
}

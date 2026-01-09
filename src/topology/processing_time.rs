use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use dashmap::{DashMap, mapref::one::RefMut};
use metrics::{Histogram, gauge, histogram};
use vector_lib::{buffers::BufferInstrumentation, stats::EwmaGauge};

use crate::{config::ComponentKey, event::EventArray};

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const EVENT_PROCESSING_TIME: &str = "event_processing_time_seconds";
const EVENT_PROCESSING_TIME_MEAN: &str = "event_processing_time_mean_seconds";
const DEFAULT_PROCESSING_TIME_EWMA_ALPHA: f64 = 0.9;

#[cfg(test)]
static METRICS_ENTRY_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub(crate) struct ProcessingTimeRecorder {
    sink_id: Arc<str>,
    metrics: DashMap<Arc<ComponentKey>, Metrics>,
    ewma_alpha: f64,
}

#[derive(Debug)]
struct Metrics {
    histogram: Histogram,
    gauge: EwmaGauge,
}

impl ProcessingTimeRecorder {
    pub(crate) fn new(sink_key: &ComponentKey, ewma_alpha: Option<f64>) -> Self {
        Self {
            sink_id: Arc::from(sink_key.id().to_owned()),
            metrics: DashMap::new(),
            ewma_alpha: ewma_alpha.unwrap_or(DEFAULT_PROCESSING_TIME_EWMA_ALPHA),
        }
    }

    fn record_events(&self, events: &EventArray) {
        let now = Utc::now();

        // Under typical use, there will tend to be runs of events within each array from the same
        // source. Instead of doing a relatively expensive lookup-or-insert in the metrics dashmap
        // for each event, cache the last entry and skip the lookup if the source has not changed.
        let mut curr_source = None;
        let mut curr_metrics = None;

        for event in events.iter_events() {
            let metadata = event.metadata();
            if let Some(ingest_timestamp) = metadata.ingest_timestamp()
                && let Some(source_id) = metadata.source_id()
                && let Some(latency_ns) = now
                    .signed_duration_since(ingest_timestamp)
                    .num_nanoseconds()
                && latency_ns >= 0
            {
                // We use a raw pointer to the contents of `source_id` as a cheap way to store and
                // test if the source has changed with a new event. The alternative would be
                // repeatedly cloning the `source_id`. Since this will be a hot path and we don't
                // actually access anything _through_ the pointer, this is a safe use of pointers.
                let source_ptr = Arc::as_ptr(source_id);
                if curr_source != Some(source_ptr) {
                    curr_metrics = Some(self.metrics_entry(source_id));
                    curr_source = Some(source_ptr);
                }

                if let Some(metrics) = curr_metrics.as_ref() {
                    metrics.record(latency_ns as f64 / NANOS_PER_SECOND);
                }
            }
        }
    }

    fn metrics_entry<'a>(
        &'a self,
        source_id: &Arc<ComponentKey>,
    ) -> RefMut<'a, Arc<ComponentKey>, Metrics> {
        #[cfg(test)]
        {
            // This is a test-only metric to track the number of times record_events calls this function.
            METRICS_ENTRY_CALLS.fetch_add(1, Ordering::Relaxed);
        }

        self.metrics
            .entry(Arc::clone(source_id))
            .or_insert_with(|| Metrics::new(&self.sink_id, source_id, self.ewma_alpha))
    }
}

impl BufferInstrumentation<EventArray> for ProcessingTimeRecorder {
    fn on_send(&self, events: &EventArray) {
        self.record_events(events);
    }
}

impl Metrics {
    fn new(sink_id: &Arc<str>, source_id: &ComponentKey, ewma_alpha: f64) -> Self {
        let sink_label = sink_id.as_ref().to_owned();
        let source_label = source_id.id().to_owned();
        let histogram = histogram!(
            EVENT_PROCESSING_TIME,
            "sink_component_id" => sink_label.clone(),
            "source_component_id" => source_label.clone(),
        );
        let gauge = gauge!(
            EVENT_PROCESSING_TIME_MEAN,
            "sink_component_id" => sink_label,
            "source_component_id" => source_label,
        );
        Self {
            histogram,
            gauge: EwmaGauge::new(gauge, Some(ewma_alpha)),
        }
    }

    fn record(&self, latency_seconds: f64) {
        self.histogram.record(latency_seconds);
        self.gauge.record(latency_seconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventArray, LogEvent};
    use chrono::Utc;

    fn make_log_event(source: &Arc<ComponentKey>) -> LogEvent {
        let mut log = LogEvent::default();
        log.metadata_mut().set_source_id(Arc::clone(source));
        log.metadata_mut().set_ingest_timestamp(Utc::now());
        log
    }

    #[test]
    fn caches_metrics_entry_by_source_run() {
        METRICS_ENTRY_CALLS.store(0, Ordering::Relaxed);

        let recorder = ProcessingTimeRecorder::new(&ComponentKey::from("sink"), None);
        let source_a = Arc::new(ComponentKey::from("source_a"));
        let source_b = Arc::new(ComponentKey::from("source_b"));
        let events = EventArray::Logs(vec![
            make_log_event(&source_a),
            make_log_event(&source_a),
            make_log_event(&source_b),
            make_log_event(&source_b),
            make_log_event(&source_a),
            make_log_event(&source_a),
        ]);

        recorder.record_events(&events);

        assert_eq!(METRICS_ENTRY_CALLS.load(Ordering::Relaxed), 3);
    }
}

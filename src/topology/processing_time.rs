#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
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
    metrics: HashMap<Arc<ComponentKey>, Metrics>,
}

impl ProcessingTimeRecorder {
    pub(crate) fn new(
        sink_key: &ComponentKey,
        sources: Vec<ComponentKey>,
        ewma_alpha: Option<f64>,
    ) -> Self {
        let ewma_alpha = ewma_alpha.unwrap_or(DEFAULT_PROCESSING_TIME_EWMA_ALPHA);
        let sink_id = Arc::from(sink_key.id().to_owned());
        let metrics = sources
            .into_iter()
            .map(|source_key| {
                let source_id = Arc::new(source_key);
                let metrics = Metrics::new(&sink_id, &source_id, ewma_alpha);
                (source_id, metrics)
            })
            .collect();
        Self { metrics }
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
                    curr_metrics = self.metrics_entry(source_id);
                    curr_source = Some(source_ptr);
                }

                if let Some(metrics) = curr_metrics {
                    metrics.record(latency_ns as f64 / NANOS_PER_SECOND);
                }
            }
        }
    }

    fn metrics_entry<'a>(&'a self, source_id: &Arc<ComponentKey>) -> Option<&'a Metrics> {
        #[cfg(test)]
        {
            // This is a test-only metric to track the number of times record_events calls this function.
            METRICS_ENTRY_CALLS.fetch_add(1, Ordering::Relaxed);
        }

        self.metrics.get(source_id)
    }
}

impl BufferInstrumentation<EventArray> for ProcessingTimeRecorder {
    fn on_send(&self, events: &EventArray) {
        self.record_events(events);
    }
}

#[derive(Debug)]
struct Metrics {
    histogram: Histogram,
    gauge: EwmaGauge,
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

        let source_a_key = ComponentKey::from("source_a");
        let source_b_key = ComponentKey::from("source_b");
        let recorder = ProcessingTimeRecorder::new(
            &ComponentKey::from("sink"),
            vec![source_a_key.clone(), source_b_key.clone()],
            None,
        );
        let source_a = Arc::new(source_a_key);
        let source_b = Arc::new(source_b_key);
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

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DedupeEventsDropped {
    pub count: u64,
}

impl InternalEvent for DedupeEventsDropped {
    fn emit(self) {
        debug!(
            message = "Events dropped.",
            count = self.count,
            intentional = true,
            reason = "Events have been found in cache for deduplication.",
        );
        counter!(
            "component_discarded_events_total",
            self.count,
            "intentional" => "true",
        );
        counter!("events_discarded_total", self.count); // Deprecated
    }
}

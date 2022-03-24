use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit(self) {
        counter!("stale_events_flushed_total", 1);
    }
}

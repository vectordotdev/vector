use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct SampleEventDiscarded;

impl InternalEvent for SampleEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

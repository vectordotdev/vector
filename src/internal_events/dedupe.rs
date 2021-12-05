use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DedupeEventDiscarded {
    pub event: crate::event::Event,
}

impl InternalEvent for DedupeEventDiscarded {
    fn emit_logs(&self) {
        trace!(message = "Encountered duplicate event; discarding.", event = ?self.event);
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

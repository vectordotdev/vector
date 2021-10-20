use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ThrottleEventDiscarded {
    pub key: Option<String>,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit_logs(&self) {
        debug!(message = "Rate limit exceeded.", key = ?self.key);
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

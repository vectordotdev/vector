use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct DedupeEventDiscarded {
    pub event: crate::Event,
}

impl InternalEvent for DedupeEventDiscarded {
    fn emit_logs(&self) {
        warn!(
            message = "Encountered duplicate event; discarding.",
            internal_log_rate_secs = 30
        );
        trace!(message = "Encountered duplicate event; discarding.", event = ?self.event);
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

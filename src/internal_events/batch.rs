use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct LargeEventDropped {
    pub length: usize,
    pub max_length: usize,
}

impl InternalEvent for LargeEventDropped {
    fn emit_logs(&self) {
        error!(
            message = "Event larger than batch max_bytes; dropping event.",
            batch_max_bytes = %self.max_length,
            length = %self.length,
            internal_log_rate_secs = 1
        );
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1,
              "reason" => "oversized");
    }
}

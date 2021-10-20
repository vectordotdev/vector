use metrics::counter;
use vector_core::internal_event::InternalEvent;

pub struct S3EventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for S3EventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64); // deprecated
        counter!("component_sent_events_total", self.count as u64);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
    }
}

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsSqsEventsSent<'a> {
    pub byte_size: usize,
    pub message_id: Option<&'a String>,
}

impl InternalEvent for AwsSqsEventsSent<'_> {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", message_id = ?self.message_id);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_events_total", 1);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
        // deprecated
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

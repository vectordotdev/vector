use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsSqsEventSent<'a> {
    pub byte_size: usize,
    pub message_id: Option<&'a String>,
}

impl InternalEvent for AwsSqsEventSent<'_> {
    fn emit_logs(&self) {
        trace!(message = "Event sent.", message_id = ?self.message_id);
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

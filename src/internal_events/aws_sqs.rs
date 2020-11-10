use super::InternalEvent;
use metrics::counter;

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
        counter!("events_processed_total", 1);
        counter!("bytes_processed_total", self.byte_size as u64);
    }
}

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
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct AwsSqsMessageGroupIdMissingKeys<'a> {
    pub keys: &'a [String],
}

impl<'a> InternalEvent for AwsSqsMessageGroupIdMissingKeys<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Keys do not exist on the event; dropping event.",
            missing_keys = ?self.keys,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("missing_keys_total", 1);
    }
}

use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsSqsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for AwsSqsEventSent {
    fn emit_metrics(&self) {
        counter!("events_processed_total", 1);
        counter!("bytes_processed_total", self.byte_size as u64);
    }
}

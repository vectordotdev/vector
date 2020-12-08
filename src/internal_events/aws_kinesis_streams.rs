use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsKinesisStreamsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisStreamsEventSent {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

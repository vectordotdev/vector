use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AwsKinesisStreamsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisStreamsEventSent {
    fn emit_metrics(&self) {
        counter!("events_processed", 1);
        counter!("bytes_processed", self.byte_size as u64);
    }
}

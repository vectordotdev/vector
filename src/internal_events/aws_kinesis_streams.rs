use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsKinesisStreamsEventSent {
    pub byte_size: usize,
}

impl InternalEvent for AwsKinesisStreamsEventSent {
    fn emit(self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

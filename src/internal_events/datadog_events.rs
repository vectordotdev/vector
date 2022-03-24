use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogEventsProcessed {
    pub byte_size: usize,
}

impl InternalEvent for DatadogEventsProcessed {
    fn emit(self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

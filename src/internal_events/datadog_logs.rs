use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DatadogLogEventProcessed {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for DatadogLogEventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

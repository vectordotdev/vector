use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InternalMetricsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalMetricsBytesReceived {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "internal",
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "internal",
        );
    }
}

use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StaticMetricsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for StaticMetricsBytesReceived {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "static",
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "static",
        );
    }
}

use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InternalMetricsBytesReceived {
    byte_size: usize,
    metric_source: &'static str,
}

impl InternalMetricsBytesReceived {
    pub const fn new_internal(byte_size: usize) -> Self {
        Self {
            byte_size,
            metric_source: "internal",
        }
    }

    pub const fn new_static(byte_size: usize) -> Self {
        Self {
            byte_size,
            metric_source: "static",
        }
    }
}

impl InternalEvent for InternalMetricsBytesReceived {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = self.metric_source,
        );
        counter!(
            "component_received_bytes_total",
            "protocol" => self.metric_source,
        )
        .increment(self.byte_size as u64);
    }
}

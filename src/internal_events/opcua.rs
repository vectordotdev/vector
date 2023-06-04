#[cfg(feature = "sources-opcua")]
use metrics::counter;
use vector_common::json_size::JsonSize;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct OpcUaBytesReceived {
    pub byte_size: JsonSize,
    pub protocol: &'static str,
}

impl InternalEvent for OpcUaBytesReceived {
    fn emit(self) {
        trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = %self.protocol,
            );
        counter!(
                "component_received_bytes_total",
                self.byte_size.get() as u64,
                "protocol" => self.protocol,
            );
    }
}

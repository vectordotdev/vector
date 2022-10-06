use metrics::{register_counter, Counter};

use super::{ByteSize, Protocol, SharedString};

crate::registered_event!(
    BytesReceived {
        protocol: SharedString,
    } => Handle {
        protocol: SharedString,
        received_bytes: Counter,
    }

    fn register(self) {
        Handle {
            received_bytes: register_counter!("component_received_bytes_total", "protocol" => self.protocol.clone()),
            protocol: self.protocol,
        }
    }

    fn emit(&self, data: ByteSize) {
        self.received_bytes.increment(data.0 as u64);
        trace!(message = "Bytes received.", byte_size = %data.0, protocol = %self.protocol);
    }
);

impl From<Protocol> for BytesReceived {
    fn from(protocol: Protocol) -> Self {
        Self {
            protocol: protocol.0,
        }
    }
}

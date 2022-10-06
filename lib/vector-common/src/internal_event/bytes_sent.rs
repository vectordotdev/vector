use metrics::{register_counter, Counter};
use tracing::trace;

use super::{ByteSize, InternalEvent, InternalEventHandle, Protocol, SharedString};

crate::registered_event!(
    BytesSent {
        byte_size: usize,
        protocol: SharedString,
    } => {
        bytes_sent: Counter = register_counter!("component_sent_bytes_total", "protocol" => self.protocol.clone()),
        protocol: SharedString = self.protocol,
    }

    fn emit(&self, byte_size: ByteSize) {
        trace!(message = "Bytes sent.", byte_size = %byte_size.0, protocol = %self.protocol);
        self.bytes_sent.increment(byte_size.0 as u64);
    }
);

impl InternalEvent for BytesSent {
    fn emit(self) {
        let bytes = self.byte_size;
        super::register(self).emit(ByteSize(bytes));
    }

    fn name(&self) -> Option<&'static str> {
        Some("BytesSent")
    }
}

impl From<Protocol> for BytesSent {
    fn from(protocol: Protocol) -> Self {
        Self {
            byte_size: 0,
            protocol: protocol.0,
        }
    }
}

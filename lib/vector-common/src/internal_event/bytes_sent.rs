use metrics::{register_counter, Counter};
use tracing::trace;

use super::{
    ByteSize, InternalEvent, InternalEventHandle, Protocol, RegisterInternalEvent, SharedString,
};

#[derive(Debug)]
pub struct BytesSent {
    pub byte_size: usize,
    pub protocol: SharedString,
}

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

impl RegisterInternalEvent for BytesSent {
    type Handle = BytesSentHandle;
    fn register(self) -> Self::Handle {
        let bytes_sent =
            register_counter!("component_sent_bytes_total", "protocol" => self.protocol.clone());
        BytesSentHandle {
            bytes_sent,
            protocol: self.protocol,
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("BytesSent")
    }
}

#[derive(Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct BytesSentHandle {
    bytes_sent: Counter,
    protocol: SharedString,
}

impl InternalEventHandle for BytesSentHandle {
    type Data = ByteSize;

    fn emit(&self, byte_size: ByteSize) {
        trace!(message = "Bytes sent.", byte_size = %byte_size.0, protocol = %self.protocol);
        self.bytes_sent.increment(byte_size.0 as u64);
    }
}

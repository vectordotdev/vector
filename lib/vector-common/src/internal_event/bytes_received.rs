use metrics::{register_counter, Counter};

use super::{ByteSize, InternalEventHandle, Protocol, RegisterInternalEvent, SharedString};

pub struct BytesReceived {
    pub protocol: SharedString,
}

impl RegisterInternalEvent for BytesReceived {
    type Handle = Handle;

    fn register(self) -> Self::Handle {
        Handle {
            received_bytes: register_counter!("component_received_bytes_total", "protocol" => self.protocol.clone()),
            protocol: self.protocol,
        }
    }
}

impl From<Protocol> for BytesReceived {
    fn from(protocol: Protocol) -> Self {
        Self {
            protocol: protocol.0,
        }
    }
}

#[derive(Clone)]
pub struct Handle {
    protocol: SharedString,
    received_bytes: Counter,
}

impl InternalEventHandle for Handle {
    type Data = ByteSize;

    fn emit(&self, data: Self::Data) {
        self.received_bytes.increment(data.0 as u64);
        trace!(message = "Bytes received.", byte_size = %data.0, protocol = %self.protocol);
    }
}

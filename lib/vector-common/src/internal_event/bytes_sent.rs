use std::borrow::Cow;

use metrics::{counter, register_counter, Counter};
use tracing::trace;

use super::{ByteSize, InternalEvent, InternalEventHandle, RegisterInternalEvent};

#[derive(Debug)]
pub struct BytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
}

impl<'a> InternalEvent for BytesSent<'a> {
    fn emit(self) {
        trace!(message = "Bytes sent.", byte_size = %self.byte_size, protocol = %self.protocol);
        counter!("component_sent_bytes_total", self.byte_size as u64,
                 "protocol" => self.protocol.to_string());
    }

    fn name(&self) -> Option<&'static str> {
        Some("BytesSent")
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct RegisteredBytesSent {
    pub protocol: Cow<'static, str>,
}

impl RegisterInternalEvent for RegisteredBytesSent {
    type Handle = BytesSentHandle;
    fn register(self) -> Self::Handle {
        let bytes_sent =
            register_counter!("component_sent_bytes_total", "protocol" => self.protocol.clone());
        BytesSentHandle {
            bytes_sent,
            protocol: self.protocol,
        }
    }
}

#[derive(Clone)]
#[allow(clippy::module_name_repetitions)]
pub struct BytesSentHandle {
    bytes_sent: Counter,
    protocol: Cow<'static, str>,
}

impl InternalEventHandle for BytesSentHandle {
    type Data = ByteSize;
    fn emit(&self, byte_size: ByteSize) {
        trace!(message = "Bytes sent.", byte_size = %byte_size.0, protocol = %self.protocol);
        self.bytes_sent.increment(byte_size.0 as u64);
    }
}

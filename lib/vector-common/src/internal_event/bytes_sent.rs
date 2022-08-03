use metrics::counter;
use tracing::trace;

use crate::internal_event::InternalEvent;

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

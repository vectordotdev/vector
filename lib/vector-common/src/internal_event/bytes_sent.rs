use metrics::counter;

use crate::internal_event::InternalEvent;

#[derive(Debug)]
pub struct BytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
}

impl<'a> InternalEvent for BytesSent<'a> {
    fn emit_logs(&self) {
        trace!(message = "Bytes sent.", byte_size = %self.byte_size, protocol = %self.protocol);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_bytes_total", self.byte_size as u64,
                 "protocol" => self.protocol.to_string());
    }

    fn name(&self) -> Option<&str> {
        Some("BytesSent")
    }
}

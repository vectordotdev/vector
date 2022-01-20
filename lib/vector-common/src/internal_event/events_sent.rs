use metrics::counter;

use crate::internal_event::InternalEvent;

#[derive(Debug)]
pub struct EventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        if self.count > 0 {
            counter!("component_sent_events_total", self.count as u64);
            counter!("events_out_total", self.count as u64);
            counter!("component_sent_event_bytes_total", self.byte_size as u64);
        }
    }

    fn name(&self) -> Option<&str> {
        Some("EventsSent")
    }
}

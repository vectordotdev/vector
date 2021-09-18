use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct StdinEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for StdinEventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Received events.", self.count);
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

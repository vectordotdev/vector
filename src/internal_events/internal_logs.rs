use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InternalLogsEventsReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalLogsEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            byte_size = %self.byte_size,
            internal = true,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", 1);
        counter!("events_in_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
    }
}

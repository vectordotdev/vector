use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InternalLogsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalLogsBytesReceived {
    fn emit_logs(&self) {
        // MUST not emit logs here to avoid an infinite log loop
    }

    fn emit_metrics(&self) {
        counter!("component_received_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct InternalLogsEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for InternalLogsEventsReceived {
    fn emit_logs(&self) {
        // MUST not emit logs here to avoid an infinite log loop
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
    }
}

use metrics::counter;
use vector_common::json_size::JsonSize;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct InternalLogsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalLogsBytesReceived {
    fn emit(self) {
        // MUST not emit logs here to avoid an infinite log loop
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => "internal",
        );
    }
}

#[derive(Debug)]
pub struct InternalLogsEventsReceived {
    pub byte_size: JsonSize,
    pub count: usize,
}

impl InternalEvent for InternalLogsEventsReceived {
    fn emit(self) {
        // MUST not emit logs here to avoid an infinite log loop
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size.get() as u64
        );
    }
}

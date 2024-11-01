use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::json_size::JsonSize;

#[derive(Debug)]
pub struct InternalLogsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalLogsBytesReceived {
    fn emit(self) {
        // MUST NOT emit logs here to avoid an infinite log loop
        counter!(
            "component_received_bytes_total",
            "protocol" => "internal",
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct InternalLogsEventsReceived {
    pub byte_size: JsonSize,
    pub count: usize,
}

impl InternalEvent for InternalLogsEventsReceived {
    fn emit(self) {
        // MUST NOT emit logs here to avoid an infinite log loop
        counter!("component_received_events_total").increment(self.count as u64);
        counter!("component_received_event_bytes_total").increment(self.byte_size.get() as u64);
    }
}

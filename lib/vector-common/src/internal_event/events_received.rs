use metrics::{counter, histogram};
use tracing::trace;

use crate::internal_event::InternalEvent;

#[derive(Debug)]
pub struct EventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsReceived {
    fn emit(self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size);

        #[allow(clippy::cast_precision_loss)]
        let fcount = self.count as f64;
        histogram!("component_received_events_count", fcount);
        counter!("component_received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
    }
}

// This should have a better name, but this event is deprecated.
// Use `EventsReceived` once the deprecated counter below is removed.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct OldEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for OldEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size,
        );
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", self.count as u64);
    }
}

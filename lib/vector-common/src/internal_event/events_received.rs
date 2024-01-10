use metrics::{register_counter, register_histogram, Counter, Histogram};
use tracing::trace;

use super::CountByteSize;

crate::registered_event!(
    EventsReceived => {
        events_count: Histogram = register_histogram!("component_received_events_count"),
        events: Counter = register_counter!("component_received_events_total"),
        event_bytes: Counter = register_counter!("component_received_event_bytes_total"),
    }

    fn emit(&self, data: CountByteSize) {
        let CountByteSize(count, byte_size) = data;

        trace!(message = "Events received.", count = %count, byte_size = %byte_size);

        #[allow(clippy::cast_precision_loss)]
        self.events_count.record(count as f64);
        self.events.increment(count as u64);
        self.event_bytes.increment(byte_size.get() as u64);
    }
);

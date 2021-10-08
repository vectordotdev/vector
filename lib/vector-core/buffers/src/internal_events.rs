use core_common::internal_event::InternalEvent;
use metrics::{counter, decrement_gauge, increment_gauge};

pub struct BufferEventsReceived {
    pub count: u64,
    pub byte_size: usize,
}

impl InternalEvent for BufferEventsReceived {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        counter!("buffer_received_events_total", self.count);
        counter!("buffer_received_bytes_total", self.byte_size as u64);
        increment_gauge!("buffer_events", self.count as f64);
        increment_gauge!("buffer_byte_size", self.byte_size as f64);
    }
}

pub struct BufferEventsSent {
    pub count: u64,
    pub byte_size: usize,
}

impl InternalEvent for BufferEventsSent {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        counter!("buffer_sent_events_total", self.count);
        counter!("buffer_sent_bytes_total", self.byte_size as u64);
        decrement_gauge!("buffer_events", self.count as f64);
        decrement_gauge!("buffer_byte_size", self.byte_size as f64);
    }
}

pub struct EventsDropped {
    pub count: u64,
}

impl InternalEvent for EventsDropped {
    fn emit_metrics(&self) {
        counter!("buffer_discarded_events_total", self.count);
    }
}

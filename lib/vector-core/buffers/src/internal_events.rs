use core_common::internal_event::InternalEvent;
use metrics::{counter, decrement_gauge, gauge, increment_gauge};

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

pub struct BufferCreated {
    pub max_size_events: Option<usize>,
    pub max_size_bytes: Option<usize>,
}

impl InternalEvent for BufferCreated {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        if let Some(max_size) = self.max_size_events {
            gauge!("buffer_max_event_size", max_size as f64);
        }
        if let Some(max_size) = self.max_size_bytes {
            gauge!("buffer_max_byte_size", max_size as f64);
        }
    }
}

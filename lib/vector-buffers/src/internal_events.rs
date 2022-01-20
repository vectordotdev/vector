use metrics::{counter, decrement_gauge, gauge, increment_gauge};
use vector_common::internal_event::InternalEvent;

pub struct BufferEventsReceived {
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsReceived {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        counter!("buffer_received_events_total", self.count, "stage" => self.idx.to_string());
        counter!("buffer_received_bytes_total", self.byte_size, "stage" => self.idx.to_string());
        increment_gauge!("buffer_events", self.count as f64, "stage" => self.idx.to_string());
        increment_gauge!("buffer_byte_size", self.byte_size as f64, "stage" => self.idx.to_string());
    }
}

pub struct BufferEventsSent {
    pub idx: usize,
    pub count: u64,
    pub byte_size: u64,
}

impl InternalEvent for BufferEventsSent {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        counter!("buffer_sent_events_total", self.count, "stage" => self.idx.to_string());
        counter!("buffer_sent_bytes_total", self.byte_size, "stage" => self.idx.to_string());
        decrement_gauge!("buffer_events", self.count as f64, "stage" => self.idx.to_string());
        decrement_gauge!("buffer_byte_size", self.byte_size as f64, "stage" => self.idx.to_string());
    }
}

pub struct EventsDropped {
    pub idx: usize,
    pub count: u64,
}

impl InternalEvent for EventsDropped {
    fn emit_metrics(&self) {
        counter!("buffer_discarded_events_total", self.count, "stage" => self.idx.to_string());
    }
}

pub struct EventsCorrupted {
    pub count: u64,
}

impl InternalEvent for EventsCorrupted {
    fn emit_metrics(&self) {
        counter!("buffer_corrupted_events_total", self.count);
    }
}

pub struct BufferCreated {
    pub idx: usize,
    pub max_size_events: Option<usize>,
    pub max_size_bytes: Option<u64>,
}

impl InternalEvent for BufferCreated {
    #[allow(clippy::cast_precision_loss)]
    fn emit_metrics(&self) {
        if let Some(max_size) = self.max_size_events {
            gauge!("buffer_max_event_size", max_size as f64, "stage" => self.idx.to_string());
        }
        if let Some(max_size) = self.max_size_bytes {
            gauge!("buffer_max_byte_size", max_size as f64, "stage" => self.idx.to_string());
        }
    }
}

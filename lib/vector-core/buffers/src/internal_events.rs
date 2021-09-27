use core_common::internal_event::InternalEvent;
use metrics::counter;

pub struct BufferEventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for BufferEventsReceived {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {
        counter!("buffer_received_events_total", self.count as u64);
        counter!("buffer_received_bytes_total", self.byte_size as u64);
    }
}

pub struct BufferEventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for BufferEventsSent {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {
        counter!("buffer_sent_events_total", self.count as u64);
        counter!("buffer_sent_bytes_total", self.byte_size as u64);
    }
}

pub struct EventsDropped {
    pub count: usize,
}

impl InternalEvent for EventsDropped {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {
        counter!("buffer_discarded_events_total", self.count as u64);
    }
}

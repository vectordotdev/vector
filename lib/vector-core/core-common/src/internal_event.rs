use metrics::counter;
use super::event_test_util;

pub trait InternalEvent {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
    if let Some(name) = event.name() {
        event_test_util::record_internal_event(name);
    }

}

#[cfg(not(test))]
pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();

}

#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_core::internal_event::emit($event)
    };
}

// Common Events

#[derive(Debug)]
pub struct EventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        if self.count > 0 {
            // events_out_total is emitted by `Acker`
            // counter!("component_sent_events_total", self.count as u64);
            // counter!("component_sent_event_bytes_total", self.byte_size as u64);
        }
    }

    fn name(&self) -> Option<&str> {Some("EventsSent")}
}

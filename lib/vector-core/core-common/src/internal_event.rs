use metrics::counter;

pub trait InternalEvent {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}
}

pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
}

#[cfg(test)]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {{
        vector_core::test_util::components::record_internal_event(stringify!($event));
        vector_core::internal_event::emit($event)
    }};
}

#[cfg(not(test))]
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
        // trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        if self.count > 0 {
            // events_out_total is emitted by `Acker`
            // counter!("component_sent_events_total", self.count as u64);
            // counter!("component_sent_event_bytes_total", self.byte_size as u64);
        }
    }
}

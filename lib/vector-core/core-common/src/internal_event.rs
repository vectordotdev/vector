use super::event_test_util;
use metrics::counter;

pub trait InternalEvent {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&str> {
        None
    }
}

// overrides the InternalEvent name
pub struct NamedInternalEvent<'a, 'b, E> {
    pub name: &'a str,
    pub event: &'b E,
}

impl<'a, 'b, E> InternalEvent for NamedInternalEvent<'a, 'b, E>
where
    E: InternalEvent,
{
    fn emit_logs(&self) {
        self.event.emit_logs()
    }
    fn emit_metrics(&self) {
        self.event.emit_metrics()
    }
    fn name(&self) -> Option<&str> {
        Some(self.name)
    }
}

#[cfg(feature = "test")]
pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
    println!("emitting: {:?}", event.name());
    if let Some(name) = event.name() {
        event_test_util::record_internal_event(name);
    }
}
#[cfg(not(feature = "test"))]
pub fn emit(event: &impl InternalEvent) {
    println!("emitting event during non-test");
    event.emit_logs();
    event.emit_metrics();
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
            counter!("component_sent_events_total", self.count as u64);
            counter!("component_sent_event_bytes_total", self.byte_size as u64);
        }
    }

    fn name(&self) -> Option<&str> {
        Some("EventsSent")
    }
}

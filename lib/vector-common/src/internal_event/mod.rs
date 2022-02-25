mod bytes_sent;
mod events_received;
mod events_sent;

pub use bytes_sent::BytesSent;
pub use events_received::EventsReceived;
pub use events_sent::{EventsSent, DEFAULT_OUTPUT};

pub trait InternalEvent {
    fn emit_logs(&self) {}
    fn emit_metrics(&self) {}

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&str> {
        None
    }
}

// Sets the name of an event if it doesn't have one
pub struct DefaultName<'a, 'b, E> {
    pub name: &'a str,
    pub event: &'b E,
}

impl<'a, 'b, E> InternalEvent for DefaultName<'a, 'b, E>
where
    E: InternalEvent,
{
    fn emit_logs(&self) {
        self.event.emit_logs();
    }
    fn emit_metrics(&self) {
        self.event.emit_metrics();
    }
    fn name(&self) -> Option<&str> {
        Some(self.event.name().unwrap_or(self.name))
    }
}

#[cfg(any(test, feature = "test"))]
pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
    if let Some(name) = event.name() {
        super::event_test_util::record_internal_event(name);
    }
}
#[cfg(not(any(test, feature = "test")))]
pub fn emit(event: &impl InternalEvent) {
    event.emit_logs();
    event.emit_metrics();
}

mod bytes_sent;
mod events_received;
mod events_sent;

pub use bytes_sent::BytesSent;
pub use events_received::EventsReceived;
pub use events_sent::{EventsSent, DEFAULT_OUTPUT};

pub trait InternalEvent: Sized {
    fn emit(self) {}

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&'static str> {
        None
    }
}

// Sets the name of an event if it doesn't have one
pub struct DefaultName<E> {
    pub name: &'static str,
    pub event: E,
}

impl<E> InternalEvent for DefaultName<E>
where
    E: InternalEvent,
{
    fn emit(self) {
        self.event.emit();
    }

    fn name(&self) -> Option<&'static str> {
        Some(self.event.name().unwrap_or(self.name))
    }
}

#[cfg(any(test, feature = "test"))]
pub fn emit(event: impl InternalEvent) {
    let name = event.name();
    event.emit();
    if let Some(name) = name {
        super::event_test_util::record_internal_event(name);
    }
}

#[cfg(not(any(test, feature = "test")))]
pub fn emit(event: impl InternalEvent) {
    event.emit();
}

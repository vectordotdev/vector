mod bytes_sent;
mod events_received;
mod events_sent;

pub use bytes_sent::BytesSent;
pub use events_received::EventsReceived;
pub use events_sent::{EventsSent, DEFAULT_OUTPUT};

pub trait InternalEvent: Sized {
    fn emit(self) {}

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&str> {
        None
    }
}

// Sets the name of an event if it doesn't have one
pub struct DefaultName<'a, E> {
    pub name: &'a str,
    pub event: E,
}

impl<'a, E> InternalEvent for DefaultName<'a, E>
where
    E: InternalEvent,
{
    fn emit(self) {
        self.event.emit();
    }

    fn name(&self) -> Option<&str> {
        Some(self.event.name().unwrap_or(self.name))
    }
}

#[cfg(any(test, feature = "test"))]
pub fn emit(event: impl InternalEvent) {
    if let Some(name) = event.name().map(ToString::to_string) {
        event.emit();
        super::event_test_util::record_internal_event(name.as_str());
    } else {
        event.emit();
    }
}

#[cfg(not(any(test, feature = "test")))]
pub fn emit(event: impl Internal_event) {
    event.emit();
}

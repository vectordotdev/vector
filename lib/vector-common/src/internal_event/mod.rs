mod bytes_received;
mod bytes_sent;
mod events_received;
mod events_sent;

pub use metrics::SharedString;

pub use bytes_received::BytesReceived;
pub use bytes_sent::BytesSent;
pub use events_received::EventsReceived;
pub use events_sent::{EventsSent, DEFAULT_OUTPUT};

pub trait InternalEvent: Sized {
    fn emit(self);

    // Optional for backwards compat until all events implement this
    fn name(&self) -> Option<&'static str> {
        None
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait RegisterInternalEvent: Sized {
    type Handle: InternalEventHandle;

    fn register(self) -> Self::Handle;

    fn name(&self) -> Option<&'static str> {
        None
    }
}

#[allow(clippy::module_name_repetitions)]
pub trait InternalEventHandle: Sized {
    type Data: Sized;
    fn emit(&self, data: Self::Data);
}

// Sets the name of an event if it doesn't have one
pub struct DefaultName<E> {
    pub name: &'static str,
    pub event: E,
}

impl<E: InternalEvent> InternalEvent for DefaultName<E> {
    fn emit(self) {
        self.event.emit();
    }

    fn name(&self) -> Option<&'static str> {
        Some(self.event.name().unwrap_or(self.name))
    }
}

impl<E: RegisterInternalEvent> RegisterInternalEvent for DefaultName<E> {
    type Handle = E::Handle;

    fn register(self) -> Self::Handle {
        self.event.register()
    }

    fn name(&self) -> Option<&'static str> {
        Some(self.event.name().unwrap_or(self.name))
    }
}

#[cfg(any(test, feature = "test"))]
pub fn emit(event: impl InternalEvent) {
    if let Some(name) = event.name() {
        super::event_test_util::record_internal_event(name);
    }
    event.emit();
}

#[cfg(not(any(test, feature = "test")))]
pub fn emit(event: impl InternalEvent) {
    event.emit();
}

#[cfg(any(test, feature = "test"))]
pub fn register<E: RegisterInternalEvent>(event: E) -> E::Handle {
    if let Some(name) = event.name() {
        super::event_test_util::record_internal_event(name);
    }
    event.register()
}

#[cfg(not(any(test, feature = "test")))]
pub fn register<E: RegisterInternalEvent>(event: E) -> E::Handle {
    event.register()
}

pub type Registered<T> = <T as RegisterInternalEvent>::Handle;

pub struct ByteSize(pub usize);

pub struct Protocol(pub SharedString);

impl Protocol {
    pub const HTTP: Protocol = Protocol(SharedString::const_str("http"));
    pub const HTTPS: Protocol = Protocol(SharedString::const_str("https"));
    pub const NONE: Protocol = Protocol(SharedString::const_str("none"));
    pub const TCP: Protocol = Protocol(SharedString::const_str("tcp"));
    pub const UDP: Protocol = Protocol(SharedString::const_str("udp"));
    pub const UNIX: Protocol = Protocol(SharedString::const_str("unix"));
}

impl From<&'static str> for Protocol {
    fn from(s: &'static str) -> Self {
        Self(SharedString::const_str(s))
    }
}

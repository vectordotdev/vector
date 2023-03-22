use serde::Deserialize;
use vector_core::event::{Event, LogEvent};

/// An event used in a test case.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum TestEvent {
    /// The event is used, as-is, without modification.
    Passthrough(EventData),

    /// The event is potentially modified by the external resource.
    ///
    /// The modification made is dependent on the external resource, but this mode is made available
    /// for when a test case wants to exercise the failure path, but cannot cause a failure simply
    /// by constructing the event in a certain way i.e. adding an invalid field, or removing a
    /// required field, or using an invalid field value, and so on.
    ///
    /// For transforms and sinks, generally, the only way to cause an error is if the event itself
    /// is malformed in some way, which can be achieved without this test event variant.
    Modified { modified: bool, event: EventData },
}

impl TestEvent {
    pub fn into_event(self) -> Event {
        match self {
            Self::Passthrough(event) => event.into_event(),
            Self::Modified { event, .. } => event.into_event(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum EventData {
    /// A log event.
    Log(String),
}

impl EventData {
    /// Converts this event data into an `Event`.
    pub fn into_event(self) -> Event {
        match self {
            Self::Log(message) => Event::Log(LogEvent::from_bytes_legacy(&message.into())),
        }
    }
}

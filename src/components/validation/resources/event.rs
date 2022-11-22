use vector_core::event::{Event, LogEvent, Metric, TraceEvent};

/// An event used in a test case.
#[derive(Clone, Debug)]
pub enum TestEvent {
    /// The event is used, as-is, without modification.
    Passthrough(Event),

    /// The event is potentially modified by the external resource.
    ///
    /// The modification made is dependent on the external resource, but this mode is made available
    /// for when a test case wants to exercise the failure path, but cannot cause a failure simply
    /// by constructing the event in a certain way i.e. adding an invalid field, or removing a
    /// required field, or using an invalud field value, and so on.
    ///
    /// For transforms and sinks, generally, the only way to cause an error is if the event itself
    /// is malformed in some way, which can be achieved without this test event variant.
    Modified(Event),
}

impl TestEvent {
    /// Creates a "modified" test event based on the given event.
    ///
    /// See `TestEvent::Modified` for more information.
    pub fn modified<E: Into<Event>>(event: E) -> Self {
        Self::Modified(event.into())
    }
}

impl From<Event> for TestEvent {
    fn from(e: Event) -> Self {
        Self::Passthrough(e)
    }
}

impl From<LogEvent> for TestEvent {
    fn from(e: LogEvent) -> Self {
        Self::Passthrough(Event::Log(e))
    }
}

impl From<Metric> for TestEvent {
    fn from(m: Metric) -> Self {
        Self::Passthrough(Event::Metric(m))
    }
}

impl From<TraceEvent> for TestEvent {
    fn from(t: TraceEvent) -> Self {
        Self::Passthrough(Event::Trace(t))
    }
}

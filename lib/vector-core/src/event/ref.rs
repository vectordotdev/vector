#![deny(missing_docs)]

use vector_common::EventDataEq;

use super::{Event, EventMetadata, LogEvent, Metric, TraceEvent};

/// A wrapper for references to inner event types, where reconstituting
/// a full `Event` from a `LogEvent` or `Metric` might be inconvenient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventRef<'a> {
    /// Reference to a `LogEvent`
    Log(&'a LogEvent),
    /// Reference to a `Metric`
    Metric(&'a Metric),
    /// Reference to a `TraceEvent`
    Trace(&'a TraceEvent),
}

impl<'a> EventRef<'a> {
    /// Extract the `LogEvent` reference in this.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `LogEvent` reference.
    pub fn as_log(self) -> &'a LogEvent {
        match self {
            Self::Log(log) => log,
            _ => panic!("Failed type coercion, {self:?} is not a log reference"),
        }
    }

    /// Convert this reference into a new `LogEvent` by cloning.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `LogEvent` reference.
    pub fn into_log(self) -> LogEvent {
        match self {
            Self::Log(log) => log.clone(),
            _ => panic!("Failed type coercion, {self:?} is not a log reference"),
        }
    }

    /// Extract the `Metric` reference in this.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `Metric` reference.
    pub fn as_metric(self) -> &'a Metric {
        match self {
            Self::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {self:?} is not a metric reference"),
        }
    }

    /// Convert this reference into a new `Metric` by cloning.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `Metric` reference.
    pub fn into_metric(self) -> Metric {
        match self {
            Self::Metric(metric) => metric.clone(),
            _ => panic!("Failed type coercion, {self:?} is not a metric reference"),
        }
    }
}

impl<'a> From<&'a Event> for EventRef<'a> {
    fn from(event: &'a Event) -> Self {
        match event {
            Event::Log(log) => EventRef::Log(log),
            Event::Metric(metric) => EventRef::Metric(metric),
            Event::Trace(trace) => EventRef::Trace(trace),
        }
    }
}

impl<'a> From<&'a LogEvent> for EventRef<'a> {
    fn from(log: &'a LogEvent) -> Self {
        Self::Log(log)
    }
}

impl<'a> From<&'a Metric> for EventRef<'a> {
    fn from(metric: &'a Metric) -> Self {
        Self::Metric(metric)
    }
}

impl<'a> From<&'a TraceEvent> for EventRef<'a> {
    fn from(trace: &'a TraceEvent) -> Self {
        Self::Trace(trace)
    }
}

impl<'a> EventDataEq<Event> for EventRef<'a> {
    fn event_data_eq(&self, that: &Event) -> bool {
        match (self, that) {
            (Self::Log(a), Event::Log(b)) => a.event_data_eq(b),
            (Self::Metric(a), Event::Metric(b)) => a.event_data_eq(b),
            (Self::Trace(a), Event::Trace(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
}

/// A wrapper for mutable references to inner event types, where reconstituting
/// a full `Event` from a `LogEvent` or `Metric` might be inconvenient.
#[derive(Debug)]
pub enum EventMutRef<'a> {
    /// Reference to a `LogEvent`
    Log(&'a mut LogEvent),
    /// Reference to a `Metric`
    Metric(&'a mut Metric),
    /// Reference to a `TraceEvent`
    Trace(&'a mut TraceEvent),
}

impl<'a> EventMutRef<'a> {
    /// Extract the `LogEvent` reference in this.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `LogEvent` reference.
    pub fn as_log(self) -> &'a LogEvent {
        match self {
            Self::Log(log) => log,
            _ => panic!("Failed type coercion, {self:?} is not a log reference"),
        }
    }

    /// Convert this reference into a new `LogEvent` by cloning.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `LogEvent` reference.
    pub fn into_log(self) -> LogEvent {
        match self {
            Self::Log(log) => log.clone(),
            _ => panic!("Failed type coercion, {self:?} is not a log reference"),
        }
    }

    /// Extract the `Metric` reference in this.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `Metric` reference.
    pub fn as_metric(self) -> &'a Metric {
        match self {
            Self::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {self:?} is not a metric reference"),
        }
    }

    /// Convert this reference into a new `Metric` by cloning.
    ///
    /// # Panics
    ///
    /// This will panic if this is not a `Metric` reference.
    pub fn into_metric(self) -> Metric {
        match self {
            Self::Metric(metric) => metric.clone(),
            _ => panic!("Failed type coercion, {self:?} is not a metric reference"),
        }
    }

    /// Access the metadata in this reference.
    pub fn metadata(&self) -> &EventMetadata {
        match self {
            Self::Log(event) => event.metadata(),
            Self::Metric(event) => event.metadata(),
            Self::Trace(event) => event.metadata(),
        }
    }

    /// Access the metadata mutably in this reference.
    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        match self {
            Self::Log(event) => event.metadata_mut(),
            Self::Metric(event) => event.metadata_mut(),
            Self::Trace(event) => event.metadata_mut(),
        }
    }
}

impl<'a> From<&'a mut Event> for EventMutRef<'a> {
    fn from(event: &'a mut Event) -> Self {
        match event {
            Event::Log(event) => event.into(),
            Event::Metric(event) => event.into(),
            Event::Trace(event) => event.into(),
        }
    }
}

impl<'a> From<&'a mut LogEvent> for EventMutRef<'a> {
    fn from(log: &'a mut LogEvent) -> Self {
        Self::Log(log)
    }
}

impl<'a> From<&'a mut Metric> for EventMutRef<'a> {
    fn from(metric: &'a mut Metric) -> Self {
        Self::Metric(metric)
    }
}

impl<'a> From<&'a mut TraceEvent> for EventMutRef<'a> {
    fn from(trace: &'a mut TraceEvent) -> Self {
        Self::Trace(trace)
    }
}

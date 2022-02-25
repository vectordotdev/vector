#![deny(missing_docs)]
//! This module contains the definitions and wrapper types for handling
//! arrays of type `Event`, in the various forms they may appear.

use std::{iter, slice, vec};

#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use vector_buffers::EventCount;

use super::{Event, EventDataEq, EventRef, LogEvent, Metric, TraceEvent};
use crate::ByteSizeOf;

/// The core trait to abstract over any type that may work as an array
/// of events. This is effectively the same as the standard
/// `IntoIterator<Item = Event>` implementations, but that would
/// conflict with the base implementation for the type aliases below.
pub trait EventContainer: ByteSizeOf {
    /// The type of `Iterator` used to turn this container into events.
    type IntoIter: Iterator<Item = Event>;

    /// The number of events in this container.
    fn len(&self) -> usize;

    /// Is this container empty?
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Turn this container into an iterator of events.
    fn into_events(self) -> Self::IntoIter;
}

impl EventContainer for Event {
    type IntoIter = iter::Once<Event>;

    fn len(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        false
    }

    fn into_events(self) -> Self::IntoIter {
        iter::once(self)
    }
}

impl EventContainer for LogEvent {
    type IntoIter = iter::Once<Event>;

    fn len(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        false
    }

    fn into_events(self) -> Self::IntoIter {
        iter::once(self.into())
    }
}

impl EventContainer for Metric {
    type IntoIter = iter::Once<Event>;

    fn len(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        false
    }

    fn into_events(self) -> Self::IntoIter {
        iter::once(self.into())
    }
}

/// The type alias for an array of `LogEvent` elements.
pub type LogArray = Vec<LogEvent>;

/// The type alias for an array of `TraceEvent` elements.
pub type TraceArray = Vec<TraceEvent>;

impl EventContainer for LogArray {
    type IntoIter = iter::Map<vec::IntoIter<LogEvent>, fn(LogEvent) -> Event>;

    fn len(&self) -> usize {
        self.len()
    }

    fn into_events(self) -> Self::IntoIter {
        self.into_iter().map(Into::into)
    }
}

/// The type alias for an array of `Metric` elements.
pub type MetricArray = Vec<Metric>;

impl EventContainer for MetricArray {
    type IntoIter = iter::Map<vec::IntoIter<Metric>, fn(Metric) -> Event>;

    fn len(&self) -> usize {
        self.len()
    }

    fn into_events(self) -> Self::IntoIter {
        self.into_iter().map(Into::into)
    }
}

/// An array of one of the `Event` variants exclusively.
#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum EventArray {
    /// An array of type `LogEvent`
    Logs(LogArray),
    /// An array of type `Metric`
    Metrics(MetricArray),
    /// An array of type `TraceEvent`
    Traces(TraceArray),
}

impl EventArray {
    /// Run the given update function over each `LogEvent` in this array.
    pub fn for_each_log(&mut self, update: impl FnMut(&mut LogEvent)) {
        if let Self::Logs(logs) = self {
            logs.iter_mut().for_each(update);
        }
    }

    /// Run the given update function over each `Metric` in this array.
    pub fn for_each_metric(&mut self, update: impl FnMut(&mut Metric)) {
        if let Self::Metrics(metrics) = self {
            metrics.iter_mut().for_each(update);
        }
    }

    /// Run the given update function over each `Trace` in this array.
    pub fn for_each_trace(&mut self, update: impl FnMut(&mut TraceEvent)) {
        if let Self::Traces(traces) = self {
            traces.iter_mut().for_each(update);
        }
    }

    /// Iterate over this array's events.
    pub fn iter_events(&self) -> impl Iterator<Item = EventRef> {
        match self {
            Self::Logs(array) => EventArrayIter::Logs(array.iter()),
            Self::Metrics(array) => EventArrayIter::Metrics(array.iter()),
            Self::Traces(array) => EventArrayIter::Traces(array.iter()),
        }
    }
}

impl From<Event> for EventArray {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(log) => Self::Logs(vec![log]),
            Event::Metric(metric) => Self::Metrics(vec![metric]),
            Event::Trace(trace) => Self::Traces(vec![trace]),
        }
    }
}

impl From<LogArray> for EventArray {
    fn from(array: LogArray) -> Self {
        Self::Logs(array)
    }
}

impl From<MetricArray> for EventArray {
    fn from(array: MetricArray) -> Self {
        Self::Metrics(array)
    }
}

impl ByteSizeOf for EventArray {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Logs(a) | Self::Traces(a) => a.allocated_bytes(),
            Self::Metrics(a) => a.allocated_bytes(),
        }
    }
}

impl EventCount for EventArray {
    fn event_count(&self) -> usize {
        match self {
            Self::Logs(a) | Self::Traces(a) => a.len(),
            Self::Metrics(a) => a.len(),
        }
    }
}

impl EventContainer for EventArray {
    type IntoIter = EventArrayIntoIter;

    fn len(&self) -> usize {
        match self {
            Self::Logs(a) | Self::Traces(a) => a.len(),
            Self::Metrics(a) => a.len(),
        }
    }

    fn into_events(self) -> Self::IntoIter {
        match self {
            Self::Logs(a) => EventArrayIntoIter::Logs(a.into_iter()),
            Self::Metrics(a) => EventArrayIntoIter::Metrics(a.into_iter()),
            Self::Traces(a) => EventArrayIntoIter::Traces(a.into_iter()),
        }
    }
}

impl EventDataEq for EventArray {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Logs(a), Self::Logs(b)) | (Self::Traces(a), Self::Traces(b)) => {
                a.event_data_eq(b)
            }
            (Self::Metrics(a), Self::Metrics(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
}

#[cfg(test)]
impl Arbitrary for EventArray {
    fn arbitrary(g: &mut Gen) -> Self {
        let len = u8::arbitrary(g) as usize;
        let choice: u8 = u8::arbitrary(g);
        // Quickcheck can't derive Arbitrary for enums, see
        // https://github.com/BurntSushi/quickcheck/issues/98
        if choice % 2 == 0 {
            let mut logs = Vec::new();
            for _ in 0..len {
                logs.push(LogEvent::arbitrary(g));
            }
            EventArray::Logs(logs)
        } else {
            let mut metrics = Vec::new();
            for _ in 0..len {
                metrics.push(Metric::arbitrary(g));
            }
            EventArray::Metrics(metrics)
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            EventArray::Logs(logs) => Box::new(logs.shrink().map(EventArray::Logs)),
            EventArray::Metrics(metrics) => Box::new(metrics.shrink().map(EventArray::Metrics)),
            EventArray::Traces(traces) => Box::new(traces.shrink().map(EventArray::Traces)),
        }
    }
}

/// The iterator type for `EventArray::iter_events`.
#[derive(Debug)]
pub enum EventArrayIter<'a> {
    /// An iterator over type `LogEvent`.
    Logs(slice::Iter<'a, LogEvent>),
    /// An iterator over type `Metric`.
    Metrics(slice::Iter<'a, Metric>),
    /// An iterator over type `Trace`.
    Traces(slice::Iter<'a, TraceEvent>),
}

impl<'a> Iterator for EventArrayIter<'a> {
    type Item = EventRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Logs(i) | Self::Traces(i) => i.next().map(EventRef::from),
            Self::Metrics(i) => i.next().map(EventRef::from),
        }
    }
}

/// The iterator type for `EventArray::into_events`.
#[derive(Debug)]
pub enum EventArrayIntoIter {
    /// An iterator over type `LogEvent`.
    Logs(vec::IntoIter<LogEvent>),
    /// An iterator over type `Metric`.
    Metrics(vec::IntoIter<Metric>),
    /// An iterator over type `LogEvent` but for Traces.
    Traces(vec::IntoIter<LogEvent>),
}

impl Iterator for EventArrayIntoIter {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Logs(i) => i.next().map(Into::into),
            Self::Metrics(i) => i.next().map(Into::into),
            Self::Traces(i) => i.next().map(Event::Trace),
        }
    }
}

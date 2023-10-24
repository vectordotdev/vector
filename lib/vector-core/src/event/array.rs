#![deny(missing_docs)]
//! This module contains the definitions and wrapper types for handling
//! arrays of type `Event`, in the various forms they may appear.

use std::{iter, slice, sync::Arc, vec};

use futures::{stream, Stream};
#[cfg(test)]
use quickcheck::{Arbitrary, Gen};
use vector_buffers::EventCount;
use vector_common::{
    byte_size_of::ByteSizeOf,
    config::ComponentKey,
    finalization::{AddBatchNotifier, BatchNotifier, EventFinalizers, Finalizable},
    json_size::JsonSize,
};

use super::{
    EstimatedJsonEncodedSizeOf, Event, EventDataEq, EventFinalizer, EventMutRef, EventRef,
    LogEvent, Metric, TraceEvent,
};

/// The type alias for an array of `LogEvent` elements.
pub type LogArray = Vec<LogEvent>;

/// The type alias for an array of `TraceEvent` elements.
pub type TraceArray = Vec<TraceEvent>;

/// The type alias for an array of `Metric` elements.
pub type MetricArray = Vec<Metric>;

/// The core trait to abstract over any type that may work as an array
/// of events. This is effectively the same as the standard
/// `IntoIterator<Item = Event>` implementations, but that would
/// conflict with the base implementation for the type aliases below.
pub trait EventContainer: ByteSizeOf + EstimatedJsonEncodedSizeOf {
    /// The type of `Iterator` used to turn this container into events.
    type IntoIter: Iterator<Item = Event>;

    /// The number of events in this container.
    fn len(&self) -> usize;

    /// Is this container empty?
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Turn this container into an iterator over `Event`.
    fn into_events(self) -> Self::IntoIter;
}

/// Turn a container into a futures stream over the contained `Event`
/// type.  This would ideally be implemented as a default method on
/// `trait EventContainer`, but the required feature (associated type
/// defaults) is still unstable.
/// See <https://github.com/rust-lang/rust/issues/29661>
pub fn into_event_stream(container: impl EventContainer) -> impl Stream<Item = Event> + Unpin {
    stream::iter(container.into_events())
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

impl EventContainer for LogArray {
    type IntoIter = iter::Map<vec::IntoIter<LogEvent>, fn(LogEvent) -> Event>;

    fn len(&self) -> usize {
        self.len()
    }

    fn into_events(self) -> Self::IntoIter {
        self.into_iter().map(Into::into)
    }
}

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
#[derive(Clone, Debug, PartialEq)]
pub enum EventArray {
    /// An array of type `LogEvent`
    Logs(LogArray),
    /// An array of type `Metric`
    Metrics(MetricArray),
    /// An array of type `TraceEvent`
    Traces(TraceArray),
}

impl EventArray {
    /// Sets the `OutputId` in the metadata for all the events in this array.
    pub fn set_output_id(&mut self, output_id: &Arc<ComponentKey>) {
        match self {
            EventArray::Logs(logs) => {
                for log in logs {
                    log.metadata_mut().set_source_id(Arc::clone(output_id));
                }
            }
            EventArray::Metrics(metrics) => {
                for metric in metrics {
                    metric.metadata_mut().set_source_id(Arc::clone(output_id));
                }
            }
            EventArray::Traces(traces) => {
                for trace in traces {
                    trace.metadata_mut().set_source_id(Arc::clone(output_id));
                }
            }
        }
    }

    /// Sets the `source_type` in the metadata for all metric events in this array.
    pub fn set_source_type(&mut self, source_type: &'static str) {
        if let EventArray::Metrics(metrics) = self {
            for metric in metrics {
                metric.metadata_mut().set_source_type(source_type);
            }
        }
    }

    /// Iterate over references to this array's events.
    pub fn iter_events(&self) -> impl Iterator<Item = EventRef> {
        match self {
            Self::Logs(array) => EventArrayIter::Logs(array.iter()),
            Self::Metrics(array) => EventArrayIter::Metrics(array.iter()),
            Self::Traces(array) => EventArrayIter::Traces(array.iter()),
        }
    }

    /// Iterate over mutable references to this array's events.
    pub fn iter_events_mut(&mut self) -> impl Iterator<Item = EventMutRef> {
        match self {
            Self::Logs(array) => EventArrayIterMut::Logs(array.iter_mut()),
            Self::Metrics(array) => EventArrayIterMut::Metrics(array.iter_mut()),
            Self::Traces(array) => EventArrayIterMut::Traces(array.iter_mut()),
        }
    }

    /// Iterate over references to the logs in this array.
    pub fn iter_logs_mut(&mut self) -> impl Iterator<Item = &mut LogEvent> {
        match self {
            Self::Logs(array) => TypedArrayIterMut(Some(array.iter_mut())),
            _ => TypedArrayIterMut(None),
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

impl From<LogEvent> for EventArray {
    fn from(log: LogEvent) -> Self {
        Event::from(log).into()
    }
}

impl From<Metric> for EventArray {
    fn from(metric: Metric) -> Self {
        Event::from(metric).into()
    }
}

impl From<TraceEvent> for EventArray {
    fn from(trace: TraceEvent) -> Self {
        Event::from(trace).into()
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

impl AddBatchNotifier for EventArray {
    fn add_batch_notifier(&mut self, batch: BatchNotifier) {
        match self {
            Self::Logs(array) => array
                .iter_mut()
                .for_each(|item| item.add_finalizer(EventFinalizer::new(batch.clone()))),
            Self::Metrics(array) => array
                .iter_mut()
                .for_each(|item| item.add_finalizer(EventFinalizer::new(batch.clone()))),
            Self::Traces(array) => array
                .iter_mut()
                .for_each(|item| item.add_finalizer(EventFinalizer::new(batch.clone()))),
        }
    }
}

impl ByteSizeOf for EventArray {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Logs(a) => a.allocated_bytes(),
            Self::Metrics(a) => a.allocated_bytes(),
            Self::Traces(a) => a.allocated_bytes(),
        }
    }
}

impl EstimatedJsonEncodedSizeOf for EventArray {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        match self {
            Self::Logs(v) => v.estimated_json_encoded_size_of(),
            Self::Traces(v) => v.estimated_json_encoded_size_of(),
            Self::Metrics(v) => v.estimated_json_encoded_size_of(),
        }
    }
}

impl EventCount for EventArray {
    fn event_count(&self) -> usize {
        match self {
            Self::Logs(a) => a.len(),
            Self::Metrics(a) => a.len(),
            Self::Traces(a) => a.len(),
        }
    }
}

impl EventContainer for EventArray {
    type IntoIter = EventArrayIntoIter;

    fn len(&self) -> usize {
        match self {
            Self::Logs(a) => a.len(),
            Self::Metrics(a) => a.len(),
            Self::Traces(a) => a.len(),
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
            (Self::Logs(a), Self::Logs(b)) => a.event_data_eq(b),
            (Self::Metrics(a), Self::Metrics(b)) => a.event_data_eq(b),
            (Self::Traces(a), Self::Traces(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
}

impl Finalizable for EventArray {
    fn take_finalizers(&mut self) -> EventFinalizers {
        match self {
            Self::Logs(a) => a.iter_mut().map(Finalizable::take_finalizers).collect(),
            Self::Metrics(a) => a.iter_mut().map(Finalizable::take_finalizers).collect(),
            Self::Traces(a) => a.iter_mut().map(Finalizable::take_finalizers).collect(),
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
            Self::Logs(i) => i.next().map(EventRef::from),
            Self::Metrics(i) => i.next().map(EventRef::from),
            Self::Traces(i) => i.next().map(EventRef::from),
        }
    }
}

/// The iterator type for `EventArray::iter_events_mut`.
#[derive(Debug)]
pub enum EventArrayIterMut<'a> {
    /// An iterator over type `LogEvent`.
    Logs(slice::IterMut<'a, LogEvent>),
    /// An iterator over type `Metric`.
    Metrics(slice::IterMut<'a, Metric>),
    /// An iterator over type `Trace`.
    Traces(slice::IterMut<'a, TraceEvent>),
}

impl<'a> Iterator for EventArrayIterMut<'a> {
    type Item = EventMutRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Logs(i) => i.next().map(EventMutRef::from),
            Self::Metrics(i) => i.next().map(EventMutRef::from),
            Self::Traces(i) => i.next().map(EventMutRef::from),
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
    /// An iterator over type `TraceEvent`.
    Traces(vec::IntoIter<TraceEvent>),
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

struct TypedArrayIterMut<'a, T>(Option<slice::IterMut<'a, T>>);

impl<'a, T> Iterator for TypedArrayIterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut().and_then(Iterator::next)
    }
}

/// Intermediate buffer for conversion of a sequence of individual
/// `Event`s into a sequence of `EventArray`s by coalescing contiguous
/// events of the same type into one array. This is used by
/// `events_into_array`.
#[derive(Debug, Default)]
pub struct EventArrayBuffer {
    buffer: Option<EventArray>,
    max_size: usize,
}

impl EventArrayBuffer {
    fn new(max_size: Option<usize>) -> Self {
        let max_size = max_size.unwrap_or(usize::MAX);
        let buffer = None;
        Self { buffer, max_size }
    }

    #[must_use]
    fn push(&mut self, event: Event) -> Option<EventArray> {
        match (event, &mut self.buffer) {
            (Event::Log(event), Some(EventArray::Logs(array))) if array.len() < self.max_size => {
                array.push(event);
                None
            }
            (Event::Metric(event), Some(EventArray::Metrics(array)))
                if array.len() < self.max_size =>
            {
                array.push(event);
                None
            }
            (Event::Trace(event), Some(EventArray::Traces(array)))
                if array.len() < self.max_size =>
            {
                array.push(event);
                None
            }
            (event, current) => current.replace(EventArray::from(event)),
        }
    }

    fn take(&mut self) -> Option<EventArray> {
        self.buffer.take()
    }
}

/// Convert the iterator over individual `Event`s into an iterator
/// over coalesced `EventArray`s.
pub fn events_into_arrays(
    events: impl IntoIterator<Item = Event>,
    max_size: Option<usize>,
) -> impl Iterator<Item = EventArray> {
    IntoEventArraysIter {
        inner: events.into_iter().fuse(),
        current: EventArrayBuffer::new(max_size),
    }
}

/// Iterator type implementing `into_arrays`
pub struct IntoEventArraysIter<I> {
    inner: iter::Fuse<I>,
    current: EventArrayBuffer,
}

impl<I: Iterator<Item = Event>> Iterator for IntoEventArraysIter<I> {
    type Item = EventArray;
    fn next(&mut self) -> Option<Self::Item> {
        for event in self.inner.by_ref() {
            if let Some(array) = self.current.push(event) {
                return Some(array);
            }
        }
        self.current.take()
    }
}

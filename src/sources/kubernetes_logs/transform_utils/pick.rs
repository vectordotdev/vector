//! Dynamically pick a tranform to use.

#![deny(missing_docs)]

use crate::{event::Event, transforms::Transform};

/// Dynamically pick a tranform to use.
///
/// The implementation would have multiple candidates and a way to determine
/// which transform to use.
pub trait Picker: Sized {
    /// Consumes an event and the picker itself, and returns a transform to
    /// use and a transformed version of the passed event.
    ///
    /// Note that the interface doesn't allow failures (i.e. it's not a
    /// [`Result`], nor [`Option`]).
    /// To handle failure modes, use provided [`Passthrough`], [`Discard`] or
    /// [`Panic`] as the returned transform, and pass the event accordingly.
    fn probe_event(self, event: Event) -> (Box<dyn Transform>, Option<Event>);
}

/// Dynamically pick a transform to use once (on first event), and then use the
/// picked transform for the rest of the events.
///
/// The first message obtained will be probed with picker, and then the system
/// will switch to using.
pub enum PickOnce<T>
where
    T: Picker + Send,
{
    /// The initial state.
    Init(Option<T>),
    /// The picked transform.
    Picked(Box<dyn Transform>),
}

impl<T> PickOnce<T>
where
    T: Picker + Send,
{
    /// Create a new [`PickOnce`] using the passed `T` as the initial state.
    pub fn new(init: T) -> Self {
        Self::Init(Some(init))
    }
}

impl<T> Transform for PickOnce<T>
where
    T: Picker + Send,
{
    fn transform(&mut self, event: Event) -> Option<Event> {
        match self {
            Self::Init(picker) => {
                let picker = picker.take().expect("impossible state");
                let (transform, event) = picker.probe_event(event);
                *self = Self::Picked(transform);
                event
            }
            Self::Picked(transform) => transform.transform(event),
        }
    }
}

/// A passthrough transform. Passes events as-is.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Passthrough;

impl Transform for Passthrough {
    fn transform(&mut self, event: Event) -> Option<Event> {
        Some(event)
    }
}

/// A discard transform. Returns `None` for every event.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Discard;

impl Transform for Discard {
    fn transform(&mut self, _event: Event) -> Option<Event> {
        None
    }
}

/// A panic transform. Just panics with the predefined message.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Panic(pub &'static str);

impl Transform for Panic {
    fn transform(&mut self, _event: Event) -> Option<Event> {
        panic!(self.0)
    }
}

/// Pick transform from an iterator by passing an event to each of the iterated
/// transport, and choosing the first that returns non-`None`.
pub struct IterPicker<T>(T)
where
    T: IntoIterator<Item = Box<dyn Transform>>;

impl<T> IterPicker<T>
where
    T: IntoIterator<Item = Box<dyn Transform>>,
{
    /// Create a new `IterPicker` from an iterator.
    pub fn new(iter: T) -> Self {
        Self(iter)
    }
}

impl<T> Picker for IterPicker<T>
where
    T: IntoIterator<Item = Box<dyn Transform>>,
{
    fn probe_event(self, event: Event) -> (Box<dyn Transform>, Option<Event>) {
        self.0
            .into_iter()
            .find_map(|mut candidate| {
                candidate
                    .transform(event.clone())
                    .map(|event| (candidate, Some(event)))
            })
            .expect("transform candidates exausted, and no suitable transform found")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_transform(transform: impl Transform + 'static) -> Box<dyn Transform> {
        Box::new(transform)
    }

    #[test]
    #[should_panic(expected = "transform candidates exausted, and no suitable transform found")]
    fn iter_picker_empty() {
        let picker = IterPicker::new(vec![]);
        picker.probe_event(Event::new_empty_log());
    }

    #[test]
    fn iter_picker_passthrough() {
        let picker = IterPicker::new(vec![box_transform(Passthrough)]);
        picker.probe_event(Event::new_empty_log());
    }

    #[test]
    #[should_panic(expected = "test panic")]
    fn iter_picker_panic() {
        let picker = IterPicker::new(vec![box_transform(Panic("test panic"))]);
        picker.probe_event(Event::new_empty_log());
    }

    #[test]
    #[should_panic(expected = "transform candidates exausted, and no suitable transform found")]
    fn iter_picker_discard() {
        let picker = IterPicker::new(vec![box_transform(Discard)]);
        picker.probe_event(Event::new_empty_log());
    }

    #[test]
    fn iter_picker_sequence() {
        let picker = IterPicker::new(vec![box_transform(Discard), box_transform(Passthrough)]);
        picker.probe_event(Event::new_empty_log());
    }
}

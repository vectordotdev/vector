use metrics::{Counter, Label};

use crate::counter;

use super::{
    Count, CounterName, InternalEvent, InternalEventHandle,
    NamedInternalEvent as NamedInternalEventTrait, RegisterInternalEvent,
};
use crate::NamedInternalEvent;

pub const INTENTIONAL: bool = true;
pub const UNINTENTIONAL: bool = false;

#[derive(Debug, NamedInternalEvent)]
pub struct ComponentEventsDropped<'a, const INTENTIONAL: bool> {
    pub count: usize,
    pub reason: &'a str,
}

impl<const INTENTIONAL: bool> InternalEvent for ComponentEventsDropped<'_, INTENTIONAL> {
    fn emit(self) {
        let count = self.count;
        self.register().emit(Count(count));
    }
}

impl<'a, const INTENTIONAL: bool> ComponentEventsDropped<'a, INTENTIONAL> {
    /// Adds arbitrary labels to the discarded events metric.
    ///
    /// The standard `intentional`, `reason`, and `count` properties are managed by this event and
    /// cannot be overridden by additional labels.
    pub fn with_tags(
        self,
        tags: impl IntoIterator<Item = Label>,
    ) -> TaggedComponentEventsDropped<'a, INTENTIONAL> {
        TaggedComponentEventsDropped {
            event: self,
            tags: tags
                .into_iter()
                .filter(|tag| !matches!(tag.key(), "intentional" | "reason" | "count"))
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct TaggedComponentEventsDropped<'a, const INTENTIONAL: bool> {
    event: ComponentEventsDropped<'a, INTENTIONAL>,
    tags: Vec<Label>,
}

impl<const INTENTIONAL: bool> NamedInternalEventTrait
    for TaggedComponentEventsDropped<'_, INTENTIONAL>
{
    fn name(&self) -> &'static str {
        self.event.name()
    }
}

impl<const INTENTIONAL: bool> InternalEvent for TaggedComponentEventsDropped<'_, INTENTIONAL> {
    fn emit(self) {
        let count = self.event.count;
        self.register().emit(Count(count));
    }
}

// TaggedComponentEventsDropped is an adapter around the ComponentEventsDropped foundation type,
// so it also has to implement RegisterInternalEvent by hand.
impl<'a, const INTENTIONAL: bool> RegisterInternalEvent
    for TaggedComponentEventsDropped<'a, INTENTIONAL>
{
    // ## skip check-validity-events ##
    type Handle = DroppedHandle<'a, INTENTIONAL>;

    fn register(mut self) -> Self::Handle {
        if self.tags.is_empty() {
            return self.event.register();
        }

        self.tags.push(Label::new(
            "intentional",
            if INTENTIONAL { "true" } else { "false" },
        ));
        DroppedHandle {
            discarded_events: counter!(CounterName::ComponentDiscardedEventsTotal, self.tags),
            reason: self.event.reason,
        }
    }
}

impl<'a, const INTENTIONAL: bool> From<&'a str> for ComponentEventsDropped<'a, INTENTIONAL> {
    fn from(reason: &'a str) -> Self {
        Self { count: 0, reason }
    }
}

// ComponentEventsDropped is the foundation type the `registered_event!` macro
// abstracts over, so we have to implement RegisterInternalEvent by hand here.
impl<'a, const INTENTIONAL: bool> RegisterInternalEvent
    for ComponentEventsDropped<'a, INTENTIONAL>
{
    // ## skip check-validity-events ##
    type Handle = DroppedHandle<'a, INTENTIONAL>;
    fn register(self) -> Self::Handle {
        Self::Handle {
            discarded_events: counter!(
                CounterName::ComponentDiscardedEventsTotal,
                "intentional" => if INTENTIONAL { "true" } else { "false" },
            ),
            reason: self.reason,
        }
    }
}

#[derive(Clone)]
pub struct DroppedHandle<'a, const INTENDED: bool> {
    discarded_events: Counter,
    reason: &'a str,
}

impl<const INTENDED: bool> InternalEventHandle for DroppedHandle<'_, INTENDED> {
    type Data = Count;
    fn emit(&self, data: Self::Data) {
        let message = "Events dropped";
        if INTENDED {
            debug!(
                message,
                intentional = INTENDED,
                count = data.0,
                reason = self.reason,
            );
        } else {
            error!(
                message,
                intentional = INTENDED,
                count = data.0,
                reason = self.reason,
            );
        }
        self.discarded_events.increment(data.0 as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_tags_exclude_standard_properties() {
        let event = ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "test",
        }
        .with_tags([
            Label::new("group", "one"),
            Label::new("source", "two"),
            Label::new("intentional", "false"),
            Label::new("reason", "override"),
            Label::new("count", "3"),
        ]);

        assert_eq!(event.name(), "ComponentEventsDropped");
        assert_eq!(event.tags.len(), 2);
        assert_eq!(event.tags[0].key(), "group");
        assert_eq!(event.tags[1].key(), "source");
    }
}

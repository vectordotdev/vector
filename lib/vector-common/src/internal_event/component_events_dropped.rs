use metrics::{Counter, Label};

use crate::counter;

use super::{Count, CounterName, InternalEvent, InternalEventHandle, RegisterInternalEvent};
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
        self.emit_with_group(None);
    }
}

impl<'a, const INTENTIONAL: bool> ComponentEventsDropped<'a, INTENTIONAL> {
    /// Emits the discarded events metric with an optional `group` label.
    pub fn emit_with_group(self, group: Option<String>) {
        #[cfg(any(test, feature = "test"))]
        crate::event_test_util::record_internal_event(<Self as super::NamedInternalEvent>::name(
            &self,
        ));

        let count = self.count;
        self.register_with_group(group).emit(Count(count));
    }

    fn register_with_group(self, group: Option<String>) -> DroppedHandle<'a, INTENTIONAL> {
        let tags = std::iter::once(Label::new(
            "intentional",
            if INTENTIONAL { "true" } else { "false" },
        ))
        .chain(group.map(|value| Label::new("group", value)))
        .collect::<Vec<_>>();

        DroppedHandle {
            discarded_events: counter!(CounterName::ComponentDiscardedEventsTotal, tags),
            reason: self.reason,
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
        self.register_with_group(None)
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

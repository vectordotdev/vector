use crate::Event;

#[derive(Default)]
pub(super) struct EventBuffer {
    pub(super) event: Option<Event>,
}

impl EventBuffer {
    pub(super) fn new(event: impl Into<Option<Event>>) -> Self {
        Self {
            event: event.into(),
        }
    }
}

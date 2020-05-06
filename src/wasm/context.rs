use crate::Event;
use std::collections::LinkedList;

#[derive(Default)]
pub(super) struct EventBuffer {
    pub(super) events: LinkedList<Event>,
}

impl EventBuffer {
    pub(super) fn new() -> Self {
        Self {
            events: Default::default(),
        }
    }
    pub(super) fn push_back(&mut self, event: Event) {
        self.events.push_back(event)
    }
    pub(crate) fn events(&self) -> &LinkedList<Event> {
        &self.events
    }
}

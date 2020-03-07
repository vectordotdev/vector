use crate::Event;
use getset::{Getters, Setters};

#[derive(Default)]
pub(super) struct EngineContext {
    pub(super) events: Vec<Event>,
}

impl EngineContext {
    pub(super) fn new(events: Vec<Event>) -> Self {
        Self {
            events,
        }
    }
}

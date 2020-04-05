use crate::Event;

#[derive(Default)]
pub(super) struct ForeignModuleContext {
    pub(super) event: Option<Event>,
}

impl ForeignModuleContext {
    pub(super) fn new(event: impl Into<Option<Event>>) -> Self {
        Self {
            event: event.into(),
        }
    }
}

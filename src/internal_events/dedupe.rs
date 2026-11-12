use vector_lib::{
    NamedInternalEvent,
    internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent, emit},
};

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct DedupeEventsDropped {
    pub count: usize,
}

impl InternalEvent for DedupeEventsDropped {
    fn emit(self) {
        emit(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "Events have been found in cache for deduplication.",
        });
    }
}

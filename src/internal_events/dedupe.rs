use crate::emit;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub struct DedupeEventsDropped {
    pub count: usize,
}

impl InternalEvent for DedupeEventsDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "Events have been found in cache for deduplication.",
        });
    }
}

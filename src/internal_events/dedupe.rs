use crate::{
    emit,
    internal_events::{ComponentEventsDropped, INTENTIONAL},
};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DedupeEventsDropped {
    pub count: u64,
}

impl InternalEvent for DedupeEventsDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "Events have been found in cache for deduplication.",
        });
        counter!("events_discarded_total", self.count); // Deprecated
    }
}

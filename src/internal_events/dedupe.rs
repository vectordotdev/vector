use metrics::counter;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

use crate::emit;

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
        counter!("events_discarded_total", self.count as u64); // Deprecated
    }
}

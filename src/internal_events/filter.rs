use crate::{
    emit,
    internal_events::{ComponentEventsDropped, INTENTIONAL},
};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct FilterEventsDropped {
    pub(crate) total: u64,
}

impl InternalEvent for FilterEventsDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.total,
            reason: "Events matched filter condition.",
        });
        counter!("events_discarded_total", self.total); // Deprecated
    }
}

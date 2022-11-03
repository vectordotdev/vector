use metrics::counter;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

use crate::emit;

#[derive(Debug)]
pub struct SampleEventDiscarded;

impl InternalEvent for SampleEventDiscarded {
    fn emit(self) {
        counter!("events_discarded_total", 1); // Deprecated.
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Sample discarded."
        })
    }
}

use crate::emit;
use metrics::counter;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub(crate) struct ThrottleEventDiscarded {
    pub key: String,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit(self) {
        debug!(message = "Rate limit exceeded.", key = ?self.key); // Deprecated.
        counter!(
            "events_discarded_total", 1,
            "key" => self.key,
        ); // Deprecated.

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Rate limit exceeded."
        })
    }
}

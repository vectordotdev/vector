use metrics::counter;
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub(crate) struct ThrottleEventDiscarded {
    pub key: String,
    pub emit_events_discarded_per_key: bool,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit(self) {
        let message = "Rate limit exceeded.";

        debug!(message, key = self.key, internal_log_rate_limit = true);
        if self.emit_events_discarded_per_key {
            counter!("events_discarded_total", 1, "key" => self.key);
        }

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: message
        })
    }
}

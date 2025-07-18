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
            // TODO: Technically, the Component Specification states that the discarded events metric
            // must _only_ have the `intentional` tag, in addition to the core tags like
            // `component_kind`, etc, and nothing else.
            //
            // That doesn't give us the leeway to specify which throttle bucket the events are being
            // discarded for... but including the key/bucket as a tag does seem useful and so I wonder
            // if we should change the specification wording? Sort of a similar situation to the
            // `error_code` tag for the component errors metric, where it's meant to be optional and
            // only specified when relevant.
            counter!("events_discarded_total", "key" => self.key).increment(1); // Deprecated.
        }

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: message
        })
    }
}

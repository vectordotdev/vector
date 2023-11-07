use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub(crate) struct ThrottleEventDiscarded {
    pub key: String,
}

impl InternalEvent for ThrottleEventDiscarded {
    fn emit(self) {
        let message = "Rate limit exceeded.";
        debug!(message, key = self.key, internal_log_rate_limit = true);

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: message
        })
    }
}

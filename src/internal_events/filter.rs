use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct FilterEventsDropped {
    pub(crate) total: u64,
}

impl InternalEvent for FilterEventsDropped {
    fn emit(self) {
        debug!(
            message = "Events dropped.",
            count = self.total,
            intentional = true,
            reason = "Events matched filter condition."
        );
        counter!(
            "events_discarded_total", self.total,
            "intentional" => "true"
        );
    }
}

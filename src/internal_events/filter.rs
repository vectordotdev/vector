use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct FilterEventDiscarded {
    pub(crate) total: u64,
}

impl InternalEvent for FilterEventDiscarded {
    fn emit(self) {
        counter!("events_discarded_total", self.total);
    }
}

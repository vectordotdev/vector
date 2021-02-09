use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct FilterEventDiscarded;

impl InternalEvent for FilterEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

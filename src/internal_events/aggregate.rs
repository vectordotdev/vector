use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AggregateEventDiscarded;

impl InternalEvent for AggregateEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

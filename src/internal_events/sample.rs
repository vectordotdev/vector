use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SampleEventDiscarded;

impl InternalEvent for SampleEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

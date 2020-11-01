use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SwimlanesEventProcessed;

impl InternalEvent for SwimlanesEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed_total", 1);
    }
}

#[derive(Debug)]
pub struct SwimlanesEventDiscarded;

impl InternalEvent for SwimlanesEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

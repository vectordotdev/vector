use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventProcessed;

impl InternalEvent for EventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub struct EventIn;

impl InternalEvent for EventIn {
    fn emit_metrics(&self) {
        counter!("events_in_total", 1);
    }
}

#[derive(Debug)]
pub struct EventOut;

impl InternalEvent for EventOut {
    fn emit_metrics(&self) {
        counter!("events_out_total", 1);
    }
}

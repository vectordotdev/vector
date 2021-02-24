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
pub struct EventZeroIn;

impl InternalEvent for EventZeroIn {
    fn emit_metrics(&self) {
        counter!("events_in_total", 0);
    }
}

#[derive(Debug)]
pub struct EventOut {
    pub count: usize,
}

impl InternalEvent for EventOut {
    fn emit_metrics(&self) {
        if self.count > 0 {
            counter!("events_out_total", self.count as u64);
        }
    }
}

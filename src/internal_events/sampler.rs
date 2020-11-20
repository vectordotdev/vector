use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SamplerEventProcessed;

impl InternalEvent for SamplerEventProcessed {
    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub struct SamplerEventDiscarded;

impl InternalEvent for SamplerEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

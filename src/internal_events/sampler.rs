use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct SamplerEventDiscarded;

impl InternalEvent for SamplerEventDiscarded {
    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
    }
}

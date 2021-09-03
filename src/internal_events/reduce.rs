use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit_metrics(&self) {
        counter!("stale_events_flushed_total", 1);
    }
}

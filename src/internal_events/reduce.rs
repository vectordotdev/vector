use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct ReduceEventProcessed;

impl InternalEvent for ReduceEventProcessed {
    fn emit_metrics(&self) {
        counter!("events_processed", 1,
            "component_kind" => "transform",
            "component_type" => "reduce",
        );
    }
}

#[derive(Debug)]
pub(crate) struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit_metrics(&self) {
        counter!("stale_events_flushed", 1,
            "component_kind" => "transform",
            "component_type" => "reduce",
        );
    }
}

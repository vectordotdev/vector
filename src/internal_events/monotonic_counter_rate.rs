use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct MonotonicCounterRateEventProcessed;

impl InternalEvent for MonotonicCounterRateEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct MonotonicCounterRateEventConverted;

impl InternalEvent for MonotonicCounterRateEventConverted {
    fn emit_logs(&self) {
        trace!(message = "Converted one event.");
    }

    fn emit_metrics(&self) {
        counter!("converted_events_total", 1);
    }
}

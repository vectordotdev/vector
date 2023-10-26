use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!("aggregate_events_recorded_total", 1);
    }
}

#[derive(Debug)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!("aggregate_flushes_total", 1);
    }
}

#[derive(Debug)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!("aggregate_failed_updates", 1);
    }
}

use metrics::counter;
use vector_config::internal_event;
use vector_lib::internal_event::InternalEvent;

#[internal_event]
#[derive(Debug)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!("aggregate_events_recorded_total").increment(1);
    }
}

#[internal_event]
#[derive(Debug)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!("aggregate_flushes_total").increment(1);
    }
}

#[internal_event]
#[derive(Debug)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!("aggregate_failed_updates").increment(1);
    }
}

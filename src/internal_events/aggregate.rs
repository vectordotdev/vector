use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent};

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!("aggregate_events_recorded_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!("aggregate_flushes_total").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!("aggregate_failed_updates").increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventDropped {
    pub reason: &'static str,
}

impl InternalEvent for AggregateEventDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: self.reason,
        });
    }
}

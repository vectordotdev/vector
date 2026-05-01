use vector_lib::internal_event::{CounterName, InternalEvent};
use vector_lib::{NamedInternalEvent, counter};

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!(CounterName::AggregateEventsRecordedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!(CounterName::AggregateFlushesTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!(CounterName::AggregateFailedUpdates).increment(1);
    }
}

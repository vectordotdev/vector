use vector_lib::{NamedInternalEvent, counter};
use vector_lib::internal_event::{InternalEvent, MetricName};

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateEventRecorded;

impl InternalEvent for AggregateEventRecorded {
    fn emit(self) {
        counter!(MetricName::AggregateEventsRecordedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateFlushed;

impl InternalEvent for AggregateFlushed {
    fn emit(self) {
        counter!(MetricName::AggregateFlushesTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AggregateUpdateFailed;

impl InternalEvent for AggregateUpdateFailed {
    fn emit(self) {
        counter!(MetricName::AggregateFailedUpdates).increment(1);
    }
}

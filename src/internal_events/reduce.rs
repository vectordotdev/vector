use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub struct ReduceStaleEventFlushed;

impl InternalEvent for ReduceStaleEventFlushed {
    fn emit(self) {
        counter!(CounterName::StaleEventsFlushedTotal).increment(1);
    }
}

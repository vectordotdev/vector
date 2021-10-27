use vector_core::{
    event::{EventFinalizers, Finalizable, LogEvent, MaybeAsLogMut},
    ByteSizeOf,
};

/// An event alongside metadata from preprocessing. This is useful for sinks
/// like Splunk HEC that process events prior to encoding.
pub struct ProcessedEvent<E, M> {
    pub event: E,
    pub metadata: M,
}

impl<E, M> MaybeAsLogMut for ProcessedEvent<E, M>
where
    E: MaybeAsLogMut,
{
    fn maybe_as_log_mut(&mut self) -> Option<&mut LogEvent> {
        self.event.maybe_as_log_mut()
    }
}

impl<E, M> Finalizable for ProcessedEvent<E, M>
where
    E: Finalizable,
{
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.event.take_finalizers()
    }
}

impl<E, M> ByteSizeOf for ProcessedEvent<E, M>
where
    E: ByteSizeOf,
    M: ByteSizeOf,
{
    fn allocated_bytes(&self) -> usize {
        self.event.allocated_bytes() + self.metadata.allocated_bytes()
    }
}

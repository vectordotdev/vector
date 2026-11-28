use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, INTENTIONAL, InternalEvent, UNINTENTIONAL, emit,
    error_stage, error_type,
};
use vector_lib::{NamedInternalEvent, counter};
use vrl::compiler::ExpressionError;

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct RemapMappingError {
    /// If set to true, the remap transform has dropped the event after a failed
    /// mapping. This internal event reflects that in its messaging.
    pub event_dropped: bool,
    pub error: String,
}

impl InternalEvent for RemapMappingError {
    fn emit(self) {
        error!(
            message = "Mapping failed with event.",
            error = ?self.error,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        if self.event_dropped {
            emit(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: "Mapping failed with event.",
            });
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct RemapMappingAbort<'a> {
    /// If set to true, the remap transform has dropped the event after an abort
    /// during mapping. This internal event reflects that in its messaging.
    pub event_dropped: bool,
    /// The message given to `abort`, or `aborted` when there is none.
    pub error: &'a ExpressionError,
}

impl InternalEvent for RemapMappingAbort<'_> {
    fn emit(self) {
        // Formatted only when debug logging is enabled: `abort` can drop every event on a hot path.
        debug!(message = "Event mapping aborted.", error = ?self.error.to_string());

        if self.event_dropped {
            emit(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "Event mapping aborted.",
            });
        }
    }
}

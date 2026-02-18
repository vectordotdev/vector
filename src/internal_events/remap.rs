use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, INTENTIONAL, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
pub struct RemapMappingError {
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
            "component_errors_total",
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        if self.event_dropped {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: "Mapping failed with event.",
            });
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct RemapMappingAbort {
    /// If set to true, the remap transform has dropped the event after an abort
    /// during mapping. This internal event reflects that in its messaging.
    pub event_dropped: bool,
}

impl InternalEvent for RemapMappingAbort {
    fn emit(self) {
        debug!("Event mapping aborted.");

        if self.event_dropped {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "Event mapping aborted.",
            });
        }
    }
}

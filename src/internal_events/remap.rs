use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{
    error_stage, error_type, ComponentEventsDropped, INTENTIONAL, UNINTENTIONAL,
};

#[derive(Debug)]
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
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        if self.event_dropped {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: "Mapping failed with event.",
            });
        }
    }
}

#[derive(Debug)]
pub struct RemapMappingAbort {
    /// If set to true, the remap transform has dropped the event after an abort
    /// during mapping. This internal event reflects that in its messaging.
    pub event_dropped: bool,
}

impl InternalEvent for RemapMappingAbort {
    fn emit(self) {
        debug!(
            message = "Event mapping aborted.",
            internal_log_rate_limit = true
        );

        if self.event_dropped {
            emit!(ComponentEventsDropped::<INTENTIONAL> {
                count: 1,
                reason: "Event mapping aborted.",
            });
        }
    }
}

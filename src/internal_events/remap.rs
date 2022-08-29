use crate::{emit, internal_events::ComponentEventsDropped};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct RemapMappingError {
    /// If set to true, the remap transform has dropped the event after a failed
    /// mapping. This internal event will reflect that in its messaging.
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
            internal_log_rate_secs = 10,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        if self.event_dropped {
            emit!(ComponentEventsDropped {
                count: 1,
                intentional: false,
                reason: "Mapping failed with event.",
            });
        }
        // deprecated
        counter!("processing_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct RemapMappingAbort {
    /// If set to true, the remap transform has dropped the event after an abort
    /// during mapping. This internal event will reflect that in its messaging.
    pub event_dropped: bool,
}

impl InternalEvent for RemapMappingAbort {
    fn emit(self) {
        debug!(
            message = "Event mapping aborted.",
            internal_log_rate_secs = 30
        );

        if self.event_dropped {
            emit!(ComponentEventsDropped {
                count: 1,
                intentional: false,
                reason: "Event mapping aborted.",
            });
        }
    }
}

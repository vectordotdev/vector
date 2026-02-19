use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};

#[derive(Debug, NamedInternalEvent)]
pub struct LargeEventDroppedError {
    pub(crate) length: usize,
    pub max_length: usize,
}

impl InternalEvent for LargeEventDroppedError {
    fn emit(self) {
        let reason = "Event larger than batch max_bytes.";
        error!(
            message = reason,
            batch_max_bytes = %self.max_length,
            length = %self.length,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_code" => "oversized",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

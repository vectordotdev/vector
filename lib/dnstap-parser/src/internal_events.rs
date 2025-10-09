use tracing::warn;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};
use vector_config::internal_event;

#[internal_event]
#[derive(Debug)]
pub(crate) struct DnstapParseWarning<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for DnstapParseWarning<E> {
    fn emit(self) {
        warn!(
            message = "Recoverable error occurred while parsing dnstap data.",
            error = %self.error,
            stage = error_stage::PROCESSING,
            error_type = error_type::PARSER_FAILED,
            internal_log_rate_limit = true,
        );
    }
}

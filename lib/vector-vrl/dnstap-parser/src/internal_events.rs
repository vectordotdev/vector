use tracing::warn;
use vector_common::{
    NamedInternalEvent,
    internal_event::{InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
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
        );
    }
}

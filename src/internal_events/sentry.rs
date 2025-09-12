use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{ComponentEventsDropped, UNINTENTIONAL, error_stage, error_type};

/// Emitted when a log event is successfully converted to a Sentry log and encoded into an envelope.
#[derive(Debug)]
pub struct SentryEventEncoded {
    pub byte_size: usize,
    pub log_count: usize,
}

impl InternalEvent for SentryEventEncoded {
    fn emit(self) {
        trace!(
            message = "Events encoded for Sentry.",
            byte_size = %self.byte_size,
            log_count = %self.log_count,
        );
        counter!("component_sent_events_total").increment(self.log_count as u64);
        counter!("component_sent_event_bytes_total").increment(self.byte_size as u64);
    }
}

/// Emitted when there's an error encoding a log event as a Sentry envelope.
#[derive(Debug)]
pub struct SentryEncodingError {
    pub error: std::io::Error,
}

impl InternalEvent for SentryEncodingError {
    fn emit(self) {
        let reason = "Failed to encode Sentry envelope.";
        error!(
            message = reason,
            error = %self.error,
            error_code = "encoding_failed",
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "encoding_failed",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

/// Emitted when the DSN parsing fails during configuration or healthcheck.
#[derive(Debug)]
pub struct SentryInvalidDsnError<E> {
    pub error: E,
    pub dsn: String,
}

impl<E: std::fmt::Display> InternalEvent for SentryInvalidDsnError<E> {
    fn emit(self) {
        error!(
            message = "Invalid Sentry DSN provided.",
            error = %self.error,
            dsn = %self.dsn,
            error_code = "invalid_dsn",
            error_type = error_type::CONFIGURATION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "invalid_dsn",
            "error_type" => error_type::CONFIGURATION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

/// Emitted when a non-log event is dropped because Sentry sink only supports log events.
#[derive(Debug)]
pub struct SentryEventTypeError {
    pub event_type: String,
}

impl InternalEvent for SentryEventTypeError {
    fn emit(self) {
        let reason = "Event type not supported by Sentry sink.";
        debug!(
            message = reason,
            event_type = %self.event_type,
            error_code = "unsupported_event_type",
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "unsupported_event_type",
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentry_event_encoded_internal_event() {
        // Just ensure the event can be created and emitted without panicking
        let event = SentryEventEncoded {
            byte_size: 1024,
            log_count: 5,
        };
        event.emit();
    }

    #[test]
    fn test_sentry_encoding_error_internal_event() {
        let event = SentryEncodingError {
            error: std::io::Error::new(std::io::ErrorKind::InvalidData, "test error"),
        };
        event.emit();
    }

    #[test]
    fn test_sentry_invalid_dsn_error_internal_event() {
        let event = SentryInvalidDsnError {
            error: "Invalid format",
            dsn: "invalid-dsn".to_string(),
        };
        event.emit();
    }

    #[test]
    fn test_sentry_event_type_error_internal_event() {
        let event = SentryEventTypeError {
            event_type: "metric".to_string(),
        };
        event.emit();
    }
}

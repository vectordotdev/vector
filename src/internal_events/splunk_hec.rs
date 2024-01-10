// ## skip check-dropped-events ##

#[cfg(feature = "sinks-splunk_hec")]
pub use self::sink::*;
#[cfg(feature = "sources-splunk_hec")]
pub use self::source::*;

#[cfg(feature = "sinks-splunk_hec")]
mod sink {
    use metrics::{counter, decrement_gauge, increment_gauge};
    use serde_json::Error;
    use vector_lib::internal_event::InternalEvent;
    use vector_lib::internal_event::{
        error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
    };

    use crate::{
        event::metric::{MetricKind, MetricValue},
        sinks::splunk_hec::common::acknowledgements::HecAckApiError,
    };

    #[derive(Debug)]
    pub struct SplunkEventEncodeError {
        pub error: vector_lib::Error,
    }

    impl InternalEvent for SplunkEventEncodeError {
        fn emit(self) {
            let reason = "Failed to encode Splunk HEC event as JSON.";
            error!(
                message = reason,
                error = ?self.error,
                error_code = "serializing_json",
                error_type = error_type::ENCODER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "serializing_json",
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
            emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
        }
    }

    #[derive(Debug)]
    pub(crate) struct SplunkInvalidMetricReceivedError<'a> {
        pub value: &'a MetricValue,
        pub kind: &'a MetricKind,
        pub error: crate::Error,
    }

    impl<'a> InternalEvent for SplunkInvalidMetricReceivedError<'a> {
        fn emit(self) {
            error!(
                message = "Invalid metric received.",
                error = ?self.error,
                error_type = error_type::INVALID_METRIC,
                stage = error_stage::PROCESSING,
                value = ?self.value,
                kind = ?self.kind,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::INVALID_METRIC,
                "stage" => error_stage::PROCESSING,
            );
            counter!(
                "component_discarded_events_total", 1,
                "error_type" => error_type::INVALID_METRIC,
                "stage" => error_stage::PROCESSING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SplunkResponseParseError {
        pub error: Error,
    }

    impl InternalEvent for SplunkResponseParseError {
        fn emit(self) {
            error!(
                message = "Unable to parse Splunk HEC response. Acknowledging based on initial 200 OK.",
                error = ?self.error,
                error_code = "invalid_response",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "invalid_response",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::SENDING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SplunkIndexerAcknowledgementAPIError {
        pub message: &'static str,
        pub error: HecAckApiError,
    }

    impl InternalEvent for SplunkIndexerAcknowledgementAPIError {
        fn emit(self) {
            error!(
                message = self.message,
                error = ?self.error,
                error_code = "indexer_ack_failed",
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "indexer_ack_failed",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::SENDING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SplunkIndexerAcknowledgementUnavailableError<E> {
        pub error: E,
    }

    impl<E: std::fmt::Display> InternalEvent for SplunkIndexerAcknowledgementUnavailableError<E> {
        fn emit(self) {
            error!(
                message = "Internal indexer acknowledgement client unavailable. Acknowledging based on initial 200 OK.",
                error = %self.error,
                error_code = "indexer_ack_unavailable",
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::SENDING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "indexer_ack_unavailable",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::SENDING,
            );
        }
    }

    pub struct SplunkIndexerAcknowledgementAckAdded;

    impl InternalEvent for SplunkIndexerAcknowledgementAckAdded {
        fn emit(self) {
            increment_gauge!("splunk_pending_acks", 1.0);
        }
    }

    pub struct SplunkIndexerAcknowledgementAcksRemoved {
        pub count: f64,
    }

    impl InternalEvent for SplunkIndexerAcknowledgementAcksRemoved {
        fn emit(self) {
            decrement_gauge!("splunk_pending_acks", self.count);
        }
    }

    pub struct SplunkEventTimestampInvalidType<'a> {
        pub r#type: &'a str,
    }

    impl<'a> InternalEvent for SplunkEventTimestampInvalidType<'a> {
        fn emit(self) {
            warn!(
                message =
                    "Timestamp was an unexpected type. Deferring to Splunk to set the timestamp.",
                invalid_type = self.r#type,
                internal_log_rate_limit = true
            );
        }
    }

    pub struct SplunkEventTimestampMissing;

    impl InternalEvent for SplunkEventTimestampMissing {
        fn emit(self) {
            warn!(
                message = "Timestamp was not found. Deferring to Splunk to set the timestamp.",
                internal_log_rate_limit = true
            );
        }
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use metrics::counter;
    use vector_lib::internal_event::InternalEvent;

    use crate::sources::splunk_hec::ApiError;
    use vector_lib::internal_event::{error_stage, error_type};

    #[derive(Debug)]
    pub struct SplunkHecRequestBodyInvalidError {
        pub error: std::io::Error,
    }

    impl InternalEvent for SplunkHecRequestBodyInvalidError {
        fn emit(self) {
            error!(
                message = "Invalid request body.",
                error = ?self.error,
                error_code = "invalid_request_body",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "invalid_request_body",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SplunkHecRequestError {
        pub(crate) error: ApiError,
    }

    impl InternalEvent for SplunkHecRequestError {
        fn emit(self) {
            error!(
                message = "Error processing request.",
                error = ?self.error,
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::RECEIVING,
                internal_log_rate_limit = true
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }
}

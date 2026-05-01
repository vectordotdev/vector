// ## skip check-dropped-events ##

#[cfg(feature = "sinks-splunk_hec")]
pub use self::sink::*;
#[cfg(feature = "sources-splunk_hec")]
pub use self::source::*;

#[cfg(feature = "sinks-splunk_hec")]
mod sink {
    use serde_json::Error;
    use vector_lib::{
        NamedInternalEvent, counter, gauge,
        internal_event::{
            ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage,
            error_type,
        },
    };

    use crate::{
        event::metric::{MetricKind, MetricValue},
        sinks::splunk_hec::common::acknowledgements::HecAckApiError,
    };

    #[derive(Debug, NamedInternalEvent)]
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
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_code" => "serializing_json",
                "error_type" => error_type::ENCODER_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
            emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub(crate) struct SplunkInvalidMetricReceivedError<'a> {
        pub value: &'a MetricValue,
        pub kind: &'a MetricKind,
        pub error: crate::Error,
    }

    impl InternalEvent for SplunkInvalidMetricReceivedError<'_> {
        fn emit(self) {
            error!(
                message = "Invalid metric received.",
                error = ?self.error,
                error_type = error_type::INVALID_METRIC,
                stage = error_stage::PROCESSING,
                value = ?self.value,
                kind = ?self.kind,
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_type" => error_type::INVALID_METRIC,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
            counter!(
                CounterName::ComponentDiscardedEventsTotal,
                "error_type" => error_type::INVALID_METRIC,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
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
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_code" => "invalid_response",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
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
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_code" => "indexer_ack_failed",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
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
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_code" => "indexer_ack_unavailable",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::SENDING,
            )
            .increment(1);
        }
    }

    #[derive(NamedInternalEvent)]
    pub struct SplunkIndexerAcknowledgementAckAdded;

    impl InternalEvent for SplunkIndexerAcknowledgementAckAdded {
        fn emit(self) {
            gauge!(CounterName::SplunkPendingAcks).increment(1.0);
        }
    }

    #[derive(NamedInternalEvent)]
    pub struct SplunkIndexerAcknowledgementAcksRemoved {
        pub count: f64,
    }

    impl InternalEvent for SplunkIndexerAcknowledgementAcksRemoved {
        fn emit(self) {
            gauge!(CounterName::SplunkPendingAcks).decrement(self.count);
        }
    }

    #[derive(NamedInternalEvent)]
    pub struct SplunkEventTimestampInvalidType<'a> {
        pub r#type: &'a str,
    }

    impl InternalEvent for SplunkEventTimestampInvalidType<'_> {
        fn emit(self) {
            warn!(
                message =
                    "Timestamp was an unexpected type. Deferring to Splunk to set the timestamp.",
                invalid_type = self.r#type
            );
        }
    }

    #[derive(NamedInternalEvent)]
    pub struct SplunkEventTimestampMissing;

    impl InternalEvent for SplunkEventTimestampMissing {
        fn emit(self) {
            warn!("Timestamp was not found. Deferring to Splunk to set the timestamp.");
        }
    }
}

#[cfg(feature = "sources-splunk_hec")]
mod source {
    use vector_lib::{
        NamedInternalEvent, counter,
        internal_event::{CounterName, InternalEvent, error_stage, error_type},
    };

    use crate::sources::splunk_hec::ApiError;

    #[derive(Debug, NamedInternalEvent)]
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
                stage = error_stage::PROCESSING
            );
            counter!(
                CounterName::ComponentErrorsTotal,
                "error_code" => "invalid_request_body",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SplunkHecRequestError {
        pub(crate) error: ApiError,
    }

    impl InternalEvent for SplunkHecRequestError {
        fn emit(self) {
            match self.error {
                ApiError::InvalidAuthorization | ApiError::MissingAuthorization => {
                    error!(
                        message = "Unauthenticated request.",
                        error = ?self.error,
                        error_type = error_type::AUTHENTICATION_FAILED,
                        stage = error_stage::RECEIVING
                    );
                    counter!(
                        CounterName::ComponentErrorsTotal,
                        "error_type" => error_type::AUTHENTICATION_FAILED,
                        "stage" => error_stage::RECEIVING,
                    )
                    .increment(1);
                }
                _ => {
                    error!(
                        message = "Error processing request.",
                        error = ?self.error,
                        error_type = error_type::REQUEST_FAILED,
                        stage = error_stage::RECEIVING
                    );
                    counter!(
                        CounterName::ComponentErrorsTotal,
                        "error_type" => error_type::REQUEST_FAILED,
                        "stage" => error_stage::RECEIVING,
                    )
                    .increment(1);
                }
            }
        }
    }
}
